#!/usr/bin/env python3
"""Run explicit live acceptance against a fresh loopback preview instance."""

import json
import secrets
import sys
import time
import urllib.error
import urllib.request

BASE = "http://127.0.0.1:18080"
MODEL = "opencode-preview-free"
UPSTREAM_MODEL = "mimo-v2.6-flash-free"


def request(method, path, body=None, token=None, stream=False):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    payload = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, payload, headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=90) as response:
            raw = response.read(2 * 1024 * 1024)
    except urllib.error.HTTPError as error:
        try:
            detail = json.loads(error.read(4096)).get("error", {})
            code = detail.get("code", "unknown") if isinstance(detail, dict) else "unknown"
        except (ValueError, AttributeError):
            code = "unknown"
        raise RuntimeError(f"{method} {path}: HTTP {error.code}, code={code}") from None
    if stream:
        frames = []
        events = []
        for line in raw.decode().splitlines():
            if line.startswith("event:"):
                events.append(line[6:].strip())
            if line.startswith("data:"):
                data = line[5:].strip()
                frames.append(data if data == "[DONE]" else json.loads(data))
        return frames, events
    return json.loads(raw) if raw else None


def main():
    password = secrets.token_urlsafe(24)
    admin = request("POST", "/api/setup", {"username": "preview-admin", "password": password})["access_token"]
    groups = request("GET", "/api/admin/groups", token=admin)["groups"]
    group_id = groups[0]["id"]
    request("POST", "/api/admin/models", {
        "model": MODEL, "display_name": "OpenCode preview fixture", "provider": "openai",
        "tags": [], "input_modalities": ["text"], "output_modalities": ["text"],
        "supports_reasoning": True, "supports_tool_calls": True,
        "visibility": "authenticated", "lifecycle": "active",
    }, admin)
    request("POST", "/api/admin/model-prices/batch", {"items": [{
        "model": MODEL, "expected_version": None, "context_window": 32768,
        "billing_mode": "free", "prices": {name: "0" for name in
            ["input", "output", "cache_read", "cache_creation_5m", "cache_creation_1h"]},
    }]}, admin)
    channel = request("POST", "/api/admin/channels", {
        "name": "OpenCode live preview", "type": "opencode", "protocol": "openai_chat",
        "base_url": None, "status": "enabled", "weight": 1, "priority": 0,
        "auto_ban": False, "pool_mode": True, "client_simulation_profile": None,
        "client_simulation_risk_accepted": False, "client_simulation_body_profile": None,
        "client_simulation_body_risk_accepted": False, "models": [MODEL],
        "group_ids": [group_id], "model_mapping": {MODEL: UPSTREAM_MODEL},
        "param_override": {}, "header_override": {}, "tag": None,
        "settings": {"opencode_entry": "inference_v1", "opencode_full_compatibility": True},
    }, admin)
    channel_id = channel["id"]
    binding = request("GET", f"/api/admin/client-profiles/channel-bindings/{channel_id}", token=admin)
    request("PUT", f"/api/admin/client-profiles/channel-bindings/{channel_id}", {
        "binding": {"key": "opencode-public", "revision": 1},
        "expected_version": binding["version"] if binding else 0,
    }, admin)
    request("POST", f"/api/admin/channels/{channel_id}/credentials", {
        "kind": "public", "secret": {"kind": "public"}, "status": "enabled",
        "multi_key_mode": None, "priority": 0, "weight": 1, "concurrency": None,
        "load_factor_micros": None, "rate_multiplier_micros": None, "schedulable": True,
        "parent_id": None, "quota_dimension": "global", "proxy_id": None,
        "oauth_provider": None, "oauth_account_key": None, "oauth_project_id": None,
    }, admin)
    key = request("POST", "/api/tokens", {
        "name": "preview-acceptance", "status": "enabled", "remain_quota": 1000000,
        "unlimited_quota": True, "expired_at": None, "model_limits": [MODEL], "allow_ips": None,
    }, admin)["api_key"]
    for _ in range(30):
        catalog = request("GET", "/v1/models", token=key)
        if any(item["id"] == MODEL for item in catalog["data"]):
            break
        time.sleep(1)
    else:
        raise RuntimeError("The published model did not reach the runtime catalog")

    first = "Remember marker AF-627. Reply exactly SAVED. Do not call tools."
    second = "What marker did I ask you to remember? Reply with the marker only. Do not call tools."
    messages = [{"role": "user", "content": first}]
    answer = request("POST", "/v1/chat/completions", {
        "model": MODEL, "messages": messages, "max_tokens": 256, "stream": False,
    }, key)
    assert answer["choices"][0]["finish_reason"] == "stop", "Chat response has no valid terminal state"
    text = answer["choices"][0]["message"]["content"]
    assert "SAVED" in text, "The first turn returned no expected content"
    print("PASS chat/json/first-turn", flush=True)
    messages += [{"role": "assistant", "content": text}, {"role": "user", "content": second}]
    answer = request("POST", "/v1/chat/completions", {
        "model": MODEL, "messages": messages, "max_tokens": 256, "stream": False,
    }, key)
    assert "AF-627" in answer["choices"][0]["message"]["content"]
    assert answer["choices"][0]["finish_reason"] == "stop"
    print("PASS chat/json/continuous-dialogue", flush=True)
    frames, _ = request("POST", "/v1/chat/completions", {
        "model": MODEL, "messages": messages, "max_tokens": 256, "stream": True,
        "stream_options": {"include_usage": True},
    }, key, True)
    chunks = [frame for frame in frames if isinstance(frame, dict)]
    assert frames[-1] == "[DONE]"
    assert "AF-627" in "".join(choice.get("delta", {}).get("content") or ""
        for chunk in chunks for choice in chunk.get("choices", []))
    assert any(choice.get("finish_reason") == "stop" for chunk in chunks for choice in chunk.get("choices", []))
    assert any(chunk.get("usage") for chunk in chunks)
    print("PASS chat/sse/continuous-dialogue", flush=True)

    prompt = "Reply exactly AF-627. Do not call tools."
    for streaming in [False, True]:
        response = request("POST", "/v1/responses", {
            "model": MODEL, "input": prompt, "max_output_tokens": 256, "stream": streaming,
        }, key, streaming)
        if streaming:
            frames, _ = response
            completed = [f["response"] for f in frames if isinstance(f, dict) and f.get("type") == "response.completed"]
            assert len(completed) == 1
            response = completed[0]
        assert response["status"] == "completed"
        assert "AF-627" in "".join(c.get("text", "") for item in response["output"] for c in item.get("content", []))
        print(f"PASS responses/{'sse' if streaming else 'json'}", flush=True)
        response = request("POST", "/v1/messages", {
            "model": MODEL, "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 256, "stream": streaming,
        }, key, streaming)
        if streaming:
            frames, events = response
            assert "message_stop" in events
            text = "".join(f.get("delta", {}).get("text", "") for f in frames if isinstance(f, dict))
            assert any(f.get("delta", {}).get("stop_reason") == "end_turn" for f in frames if isinstance(f, dict))
        else:
            assert response["stop_reason"] == "end_turn"
            text = "".join(c.get("text", "") for c in response["content"])
        assert "AF-627" in text
        print(f"PASS messages/{'sse' if streaming else 'json'}", flush=True)
        action = "streamGenerateContent?alt=sse" if streaming else "generateContent"
        response = request("POST", f"/v1beta/models/{MODEL}:{action}", {
            "contents": [{"role": "user", "parts": [{"text": prompt}]}],
            "generationConfig": {"maxOutputTokens": 256},
        }, key, streaming)
        frames = response[0] if streaming else [response]
        candidates = [candidate for frame in frames if isinstance(frame, dict) for candidate in frame.get("candidates", [])]
        assert any(candidate.get("finishReason") == "STOP" for candidate in candidates)
        assert "AF-627" in "".join(part.get("text", "") for candidate in candidates
            for part in candidate.get("content", {}).get("parts", []) if not part.get("thought"))
        print(f"PASS gemini/{'sse' if streaming else 'json'}", flush=True)
    print("Live acceptance passed: one fixed free upstream, four downstream protocols, JSON/SSE and continuous dialogue.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, RuntimeError, urllib.error.URLError, KeyError, TypeError) as error:
        print(f"Live acceptance failed: {error}", file=sys.stderr)
        sys.exit(1)
