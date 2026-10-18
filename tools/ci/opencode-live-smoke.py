#!/usr/bin/env python3
"""Run explicit live acceptance against a fresh loopback preview instance."""

import http.client
import json
import os
import secrets
import sys
import time
import urllib.error
import urllib.request

BASE = "http://127.0.0.1:18080"
MODEL = "opencode-preview-free"
UPSTREAM_MODEL = "mimo-v2.6-flash-free"
ADMIN_TOKEN = None


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
    except (http.client.HTTPException, OSError) as error:
        raise RuntimeError(f"{method} {path}: incomplete transport ({type(error).__name__})") from None
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
    global ADMIN_TOKEN
    access = os.environ.get("OPENCODE_LIVE_ACCESS", "public")
    upstream_key = os.environ.get("OPENCODE_LIVE_API_KEY", "") if access == "api_key" else ""
    if access not in ["public", "api_key"] or (access == "api_key" and not upstream_key):
        raise RuntimeError("API Key acceptance requires the configured repository secret")
    password = secrets.token_urlsafe(24)
    admin = request("POST", "/api/setup", {"username": "preview-admin", "password": password})["access_token"]
    ADMIN_TOKEN = admin
    request("PUT", "/api/admin/debug-trace-settings", {
        "enabled": True, "sample_per_million": 1000000, "retention_hours": 1,
        "capture_headers": False, "capture_bodies": True, "max_body_bytes": 65536,
    }, admin)
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
        "param_override": {"temperature": 0}, "header_override": {}, "tag": None,
        "settings": {"opencode_entry": "inference_v1", "opencode_full_compatibility": True},
    }, admin)
    channel_id = channel["id"]
    binding = request("GET", f"/api/admin/client-profiles/channel-bindings/{channel_id}", token=admin)
    request("PUT", f"/api/admin/client-profiles/channel-bindings/{channel_id}", {
        "binding": {"key": "opencode" if access == "api_key" else "opencode-public", "revision": 1},
        "expected_version": binding["version"] if binding else 0,
    }, admin)
    request("POST", f"/api/admin/channels/{channel_id}/credentials", {
        "kind": access,
        "secret": {"kind": "api_key", "api_key": upstream_key} if access == "api_key" else {"kind": "public"},
        "status": "enabled",
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
    failures = []
    for protocol in ["chat", "responses", "messages", "gemini"]:
        for streaming in [False, True]:
            name = f"{protocol}/{'sse' if streaming else 'json'}"
            messages = [{"role": "user", "content": first}]
            try:
                text = conversation_turn(protocol, streaming, messages, key)
                assert "SAVED" in text, "The first turn returned no expected content"
                print(f"PASS {name}/first-turn", flush=True)
                messages += [{"role": "assistant", "content": text}, {"role": "user", "content": second}]
                text = conversation_turn(protocol, streaming, messages, key)
                assert "AF-627" in text, "The second turn did not retain conversation history"
                print(f"PASS {name}/continuous-dialogue", flush=True)
            except (AssertionError, RuntimeError, urllib.error.URLError, KeyError, TypeError) as error:
                failures.append(name)
                print(f"FAIL {name}: {error}", flush=True)
                report_failure()
    if failures:
        raise RuntimeError("Failed protocol cases: " + ", ".join(failures))
    print(f"Live acceptance passed ({access}): one fixed free upstream, four downstream protocols, JSON/SSE and two turns in every case.", flush=True)


def conversation_turn(protocol, streaming, messages, key):
    if protocol == "chat":
        response = request("POST", "/v1/chat/completions", {
            "model": MODEL, "messages": messages, "max_tokens": 256, "stream": streaming,
            "stream_options": {"include_usage": True} if streaming else None,
        } if streaming else {
            "model": MODEL, "messages": messages, "max_tokens": 256, "stream": False,
        }, key, streaming)
        if not streaming:
            assert response["choices"][0]["finish_reason"] == "stop"
            assert response.get("usage"), "Chat response has no usage"
            return response["choices"][0]["message"]["content"]
        frames, _ = response
        chunks = [frame for frame in frames if isinstance(frame, dict)]
        assert frames[-1] == "[DONE]", "Chat stream has no terminal marker"
        finished = [choice["finish_reason"] for chunk in chunks for choice in chunk.get("choices", [])
                    if choice.get("finish_reason")]
        assert finished == ["stop"], "Chat stream repeated or missed its terminal state"
        assert any(chunk.get("usage") for chunk in chunks), "Chat stream has no usage"
        return "".join(choice.get("delta", {}).get("content") or ""
                       for chunk in chunks for choice in chunk.get("choices", []))
    if protocol == "responses":
        response = request("POST", "/v1/responses", {
            "model": MODEL, "input": messages, "max_output_tokens": 256, "stream": streaming,
        }, key, streaming)
        if streaming:
            frames, _ = response
            completed = [f["response"] for f in frames if isinstance(f, dict) and f.get("type") == "response.completed"]
            assert len(completed) == 1, "Responses stream has no unique completed response"
            response = completed[0]
        assert response["status"] == "completed"
        assert response.get("usage"), "Responses response has no usage"
        return "".join(c.get("text", "") for item in response["output"] for c in item.get("content", []))
    if protocol == "messages":
        response = request("POST", "/v1/messages", {
            "model": MODEL, "messages": messages, "max_tokens": 256, "stream": streaming,
        }, key, streaming)
        if not streaming:
            assert response["stop_reason"] == "end_turn"
            assert response.get("usage"), "Messages response has no usage"
            return "".join(c.get("text", "") for c in response["content"])
        frames, events = response
        assert events.count("message_stop") == 1, "Messages stream has no unique terminal event"
        assert any(f.get("delta", {}).get("stop_reason") == "end_turn" for f in frames if isinstance(f, dict))
        assert any(f.get("usage") for f in frames if isinstance(f, dict)), "Messages stream has no usage"
        return "".join(f.get("delta", {}).get("text", "") for f in frames if isinstance(f, dict))
    action = "streamGenerateContent?alt=sse" if streaming else "generateContent"
    response = request("POST", f"/v1beta/models/{MODEL}:{action}", {
        "contents": [{"role": "model" if m["role"] == "assistant" else "user", "parts": [{"text": m["content"]}]}
                     for m in messages],
        "generationConfig": {"maxOutputTokens": 256},
    }, key, streaming)
    frames = response[0] if streaming else [response]
    candidates = [c for frame in frames if isinstance(frame, dict) for c in frame.get("candidates", [])]
    assert any(c.get("finishReason") == "STOP" for c in candidates), "Gemini response has no stop state"
    assert any(frame.get("usageMetadata") for frame in frames if isinstance(frame, dict)), "Gemini response has no usage"
    return "".join(part.get("text", "") for c in candidates
                   for part in c.get("content", {}).get("parts", []) if not part.get("thought"))


def report_failure():
    if not ADMIN_TOKEN:
        return
    try:
        for _ in range(5):
            traces = request("GET", "/api/admin/debug-traces?limit=5", token=ADMIN_TOKEN)["traces"]
            if traces:
                break
            time.sleep(1)
        for trace in traces:
            detail = request("GET", f"/api/admin/debug-traces/{trace['id']}", token=ADMIN_TOKEN)
            print("Trace:", json.dumps({
                "outcome": trace["outcome"], "attempt_count": trace["attempt_count"],
                "attempts": [{name: item.get(name) for name in [
                    "outcome", "failure_kind", "upstream_status", "response_status", "response_streamed",
                ]} for item in detail["attempts"]],
            }), flush=True)
            snapshots = request("POST", f"/api/admin/debug-traces/{trace['id']}/snapshots", {
                "scope": "bodies",
            }, ADMIN_TOKEN)
            for attempt in snapshots["attempts"]:
                response = attempt.get("response")
                if isinstance(response, dict):
                    print("Upstream response:", json.dumps(response, ensure_ascii=False)[:2000], flush=True)
    except (RuntimeError, urllib.error.URLError, KeyError, TypeError) as error:
        print(f"Diagnostic metadata unavailable: {error}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, RuntimeError, urllib.error.URLError, KeyError, TypeError) as error:
        print(f"Live acceptance failed: {error}", file=sys.stderr)
        report_failure()
        sys.exit(1)
