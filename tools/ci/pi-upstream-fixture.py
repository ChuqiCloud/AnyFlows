#!/usr/bin/env python3
"""Four deterministic protocol fixtures for real Pi acceptance in a CI preview.

The preview is started with its HTTP proxy set to 127.0.0.1:18081 and trusted
proxy DNS enabled. Public *.pi-fixture.example destinations terminate here;
there is no forwarding to a paid model and no change to client request bodies.
"""

import json
from pathlib import Path
import re
import secrets
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

PROTOCOLS = {
    "chat": ("openai", "openai_chat", "/v1/chat/completions"),
    "responses": ("openai", "openai_responses", "/v1/responses"),
    "messages": ("anthropic", "anthropic", "/v1/messages"),
    "gemini": ("gemini", "gemini", "/v1beta/models/"),
}
CREATED = 1700000000


def model_for(target):
    return "pi-matrix-" + target


def start_fixture(trace_dir):
    server = ThreadingHTTPServer(("127.0.0.1", 18081), Fixture)
    server.trace_dir = Path(trace_dir)
    server.active = None
    server.hits = []
    server.failures = []
    server.lock = threading.Lock()
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def seed_preview(seed, fixture):
    """Publish four model aliases, each with exactly one usable protocol channel."""
    api = seed.request
    password = secrets.token_urlsafe(24)
    admin = api("POST", "/api/setup", {"username": "pi-matrix-admin", "password": password})["access_token"]
    groups = api("GET", "/api/admin/groups", token=admin)["groups"]
    group_id = groups[0]["id"]
    models = []
    for target, (kind, protocol, _) in PROTOCOLS.items():
        model = model_for(target)
        models.append(model)
        api("POST", "/api/admin/models", {
            "model": model, "display_name": "Pi controlled " + target, "provider": kind,
            "tags": [], "input_modalities": ["text"], "output_modalities": ["text"],
            "supports_reasoning": True, "supports_tool_calls": True,
            "visibility": "authenticated", "lifecycle": "active",
        }, admin)
        api("POST", "/api/admin/model-prices/batch", {"items": [{
            "model": model, "expected_version": None, "context_window": 1_000_000,
            "billing_mode": "free", "prices": {name: "0" for name in
                ["input", "output", "cache_read", "cache_creation_5m", "cache_creation_1h"]},
        }]}, admin)
        base_path = "/v1beta" if target == "gemini" else "/v1"
        channel = api("POST", "/api/admin/channels", {
            "name": "Pi matrix " + target, "type": kind, "protocol": protocol,
            "base_url": "http://" + target + ".pi-fixture.example" + base_path,
            "status": "enabled", "weight": 1, "priority": 0, "auto_ban": False,
            "pool_mode": True, "client_simulation_profile": None,
            "client_simulation_risk_accepted": False, "client_simulation_body_profile": None,
            "client_simulation_body_risk_accepted": False, "models": [model],
            "group_ids": [group_id], "model_mapping": {model: model},
            "param_override": {}, "header_override": {}, "tag": "pi-controlled-fixture", "settings": {},
        }, admin)
        api("POST", f"/api/admin/channels/{channel['id']}/credentials", {
            "kind": "api_key", "secret": {"kind": "api_key", "api_key": secrets.token_urlsafe(24)},
            "status": "enabled", "multi_key_mode": None, "priority": 0, "weight": 1,
            "concurrency": None, "load_factor_micros": None, "rate_multiplier_micros": None,
            "schedulable": True, "parent_id": None, "quota_dimension": "global", "proxy_id": None,
            "oauth_provider": None, "oauth_account_key": None, "oauth_project_id": None,
        }, admin)
    key = api("POST", "/api/tokens", {
        "name": "pi-controlled-acceptance", "status": "enabled", "remain_quota": 1_000_000,
        "unlimited_quota": True, "expired_at": None, "model_limits": models, "allow_ips": None,
    }, admin)["api_key"]
    for _ in range(30):
        published = {item["id"] for item in api("GET", "/v1/models", token=key)["data"]}
        if set(models).issubset(published):
            return key
        time.sleep(1)
    raise RuntimeError("Four controlled models did not reach the runtime catalog")


def begin_turn(server, source, target, thinking, turn, fixture_path):
    server.active = {"source": source, "target": target, "thinking": thinking,
                     "turn": turn, "fixture_path": str(fixture_path), "start": len(server.hits),
                     "failure_start": len(server.failures)}


def verify_turn(server):
    case = server.active
    hits = server.hits[case["start"]:]
    expected_count = 2 if case["turn"] == 3 else 1
    if len(hits) != expected_count:
        raise AssertionError(f"Controlled upstream expected {expected_count} requests, observed {len(hits)}")
    if any(hit["target"] != case["target"] or not hit["valid"] for hit in hits):
        raise AssertionError("Controlled upstream route or history assertion failed")
    if server.failures[case["failure_start"]:]:
        raise AssertionError("Controlled upstream encountered a fixture assertion failure")


class Fixture(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_POST(self):
        identity = secrets.token_hex(8)
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        case = self.server.active
        try:
            if not case:
                raise AssertionError("No active fixture case")
            url = urlsplit(self.path)
            target = (url.hostname or self.headers.get("Host", "").split(":")[0]).split(".")[0]
            if target not in PROTOCOLS or target != case["target"]:
                raise AssertionError("Selected an unexpected upstream fixture")
            expected_path = PROTOCOLS[target][2]
            path = url.path if url.scheme else self.path.split("?", 1)[0]
            if not path.startswith(expected_path) or (target != "gemini" and path != expected_path):
                raise AssertionError("Upstream protocol endpoint does not match the selected target")
            value = json.loads(raw)
            validate_request(target, value, case)
            # Store original upstream bytes only in runner-private temporary files.
            name = f"{case['source']}-to-{target}-{case['thinking']}-{case['turn']}-{identity}"
            (self.server.trace_dir / (name + ".upstream.request.bin")).write_bytes(raw)
            tool_result = has_tool_result(target, value)
            if case["turn"] == 3 and not tool_result:
                text = None
                call = {"id": "call_pi_matrix_read", "name": "read", "arguments": {"path": case["fixture_path"]}}
            else:
                text = {1: "SAVED", 2: "PI_MEMORY_731", 3: "PI_LIVE_FILE_482", 4: "PI_MEMORY_731 PI_LIVE_FILE_482"}[case["turn"]]
                call = None
            streaming = (":streamGenerateContent" in path) if target == "gemini" else value.get("stream") is True
            thinking = case["thinking"] != "off"
            body, content_type = response(target, text, call, streaming, thinking)
            (self.server.trace_dir / (name + ".upstream.response.bin")).write_bytes(body)
            self.server.hits.append({"target": target, "path": path, "valid": True,
                                     "stream": streaming, "tool": call is not None,
                                     "tool_result": tool_result})
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            # Fragment the wire at non-event boundaries to exercise incremental parsing.
            for index in range(0, len(body), 37):
                self.wfile.write(body[index:index + 37])
                self.wfile.flush()
        except (AssertionError, ValueError, TypeError, KeyError) as error:
            self.server.failures.append({"kind": type(error).__name__, "message": str(error)})
            self.server.hits.append({"target": "invalid", "valid": False})
            body = json.dumps({"error": {"type": "fixture_contract", "message": str(error)}}).encode()
            self.send_response(422)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)
        finally:
            self.close_connection = True


def validate_request(target, value, case):
    key = "contents" if target == "gemini" else "input" if target == "responses" else "messages"
    if key not in value:
        raise AssertionError("Expected target protocol request structure is absent")
    if target != "gemini" and value.get("model") != model_for(target):
        raise AssertionError("Model alias selected the wrong controlled channel")
    # System prompts contain neither acceptance marker. Both must originate in history.
    history = json.dumps(value[key], ensure_ascii=False)
    if "PI_MEMORY_731" not in history:
        raise AssertionError("First user memory marker was lost")
    if case["turn"] >= 2 and "SAVED" not in history:
        raise AssertionError("Previous complete assistant output was lost")
    if case["turn"] == 4 and "PI_LIVE_FILE_482" not in history:
        raise AssertionError("Completed tool output was lost in the next user turn")
    if case["turn"] == 3:
        tools = json.dumps(value.get("tools", []))
        if '"read"' not in tools or '"path"' not in tools:
            raise AssertionError("Read tool definition or argument schema was lost")


def has_tool_result(target, value):
    if target == "chat":
        return any(item.get("role") == "tool" for item in value.get("messages", []))
    if target == "responses":
        return any(item.get("type") == "function_call_output" for item in value.get("input", []) if isinstance(item, dict))
    if target == "messages":
        return any(part.get("type") == "tool_result" for item in value.get("messages", [])
                   for part in item.get("content", []) if isinstance(part, dict))
    return any("functionResponse" in part for item in value.get("contents", []) for part in item.get("parts", []))


def event(kind, value):
    return ("event: " + kind + "\ndata: " + json.dumps(value, separators=(",", ":")) + "\n\n").encode()


def data(value):
    return ("data: " + json.dumps(value, separators=(",", ":")) + "\n\n").encode()


def response(target, text, call, stream, thinking):
    model = model_for(target)
    signature = "cGktbWF0cml4LXNpZ25hdHVyZQ=="
    reasoning = "Controlled fixture reasoning."
    if target == "chat":
        message = {"role": "assistant", "content": text}
        if thinking:
            message["reasoning_content"] = reasoning
        if call:
            message["tool_calls"] = [{"id": call["id"], "type": "function", "function": {"name": call["name"], "arguments": json.dumps(call["arguments"])}}]
        finish = "tool_calls" if call else "stop"
        usage = {"prompt_tokens": 41, "completion_tokens": 3, "total_tokens": 44}
        if not stream:
            return json.dumps({"id": "chatcmpl-pi-matrix", "object": "chat.completion", "created": CREATED,
                               "model": model, "choices": [{"index": 0, "message": message, "finish_reason": finish}], "usage": usage}).encode(), "application/json"
        def chunk(delta, stop=None, used=None):
            item = {"id": "chatcmpl-pi-matrix", "object": "chat.completion.chunk", "created": CREATED,
                    "model": model, "choices": [{"index": 0, "delta": delta, "finish_reason": stop}]}
            if used:
                item["usage"] = used
            return data(item)
        body = chunk({"role": "assistant"})
        if thinking:
            body += chunk({"reasoning_content": reasoning})
        if call:
            body += chunk({"tool_calls": [{"index": 0, **message["tool_calls"][0]}]})
        else:
            body += chunk({"content": text})
        body += chunk({}, finish) + data({"id": "chatcmpl-pi-matrix", "object": "chat.completion.chunk", "created": CREATED, "model": model, "choices": [], "usage": usage}) + b"data: [DONE]\n\n"
        return body, "text/event-stream"
    if target == "responses":
        output = []
        if thinking:
            output.append({"type": "reasoning", "id": "rs_pi_matrix", "summary": [{"type": "summary_text", "text": reasoning}]})
        if call:
            output.append({"type": "function_call", "id": "fc_pi_matrix", "call_id": call["id"], "name": call["name"], "arguments": json.dumps(call["arguments"]), "status": "completed"})
        else:
            output.append({"type": "message", "id": "msg_pi_matrix", "role": "assistant", "status": "completed", "phase": "final_answer", "content": [{"type": "output_text", "text": text, "annotations": []}]})
        full = {"id": "resp_pi_matrix", "object": "response", "created_at": CREATED, "model": model,
                "status": "completed", "output": output, "error": None, "incomplete_details": None,
                "usage": {"input_tokens": 41, "output_tokens": 3, "total_tokens": 44}}
        if not stream:
            return json.dumps(full).encode(), "application/json"
        start = {**full, "status": "in_progress", "output": [], "usage": None}
        sequence = 0
        body = b""
        def emit(kind, payload):
            nonlocal sequence
            sequence += 1
            return event(kind, {"type": kind, "sequence_number": sequence, **payload})
        body += emit("response.created", {"response": start})
        body += emit("response.in_progress", {"response": start})
        for index, item in enumerate(output):
            initial = {**item}
            if item["type"] == "message":
                initial.update(status="in_progress", content=[])
            elif item["type"] == "function_call":
                initial.update(status="in_progress", arguments="")
            elif item["type"] == "reasoning":
                initial["summary"] = []
            body += emit("response.output_item.added", {"output_index": index, "item": initial})
            common = {"item_id": item["id"], "output_index": index}
            if item["type"] == "message":
                part = item["content"][0]
                body += emit("response.content_part.added", {**common, "content_index": 0, "part": {**part, "text": ""}})
                body += emit("response.output_text.delta", {**common, "content_index": 0, "delta": text})
                body += emit("response.output_text.done", {**common, "content_index": 0, "text": text})
                body += emit("response.content_part.done", {**common, "content_index": 0, "part": part})
            elif item["type"] == "function_call":
                body += emit("response.function_call_arguments.delta", {**common, "delta": item["arguments"]})
                body += emit("response.function_call_arguments.done", {**common, "arguments": item["arguments"]})
            else:
                part = item["summary"][0]
                body += emit("response.reasoning_summary_part.added", {**common, "summary_index": 0, "part": {"type": "summary_text", "text": ""}})
                body += emit("response.reasoning_summary_text.delta", {**common, "summary_index": 0, "delta": reasoning})
                body += emit("response.reasoning_summary_text.done", {**common, "summary_index": 0, "text": reasoning})
                body += emit("response.reasoning_summary_part.done", {**common, "summary_index": 0, "part": part})
            body += emit("response.output_item.done", {"output_index": index, "item": item})
        body += emit("response.completed", {"response": full})
        return body, "text/event-stream"
    if target == "messages":
        content = []
        if thinking:
            content.append({"type": "thinking", "thinking": reasoning, "signature": signature})
        content.append({"type": "tool_use", "id": call["id"], "name": call["name"], "input": call["arguments"]} if call else {"type": "text", "text": text})
        stop = "tool_use" if call else "end_turn"
        full = {"id": "msg_pi_matrix", "type": "message", "role": "assistant", "model": model,
                "content": content, "stop_reason": stop, "stop_sequence": None,
                "usage": {"input_tokens": 41, "output_tokens": 3}}
        if not stream:
            return json.dumps(full).encode(), "application/json"
        start = {**full, "content": [], "stop_reason": None, "usage": {"input_tokens": 41, "output_tokens": 0}}
        body = event("message_start", {"type": "message_start", "message": start})
        for index, part in enumerate(content):
            initial = {**part}
            if part["type"] == "thinking":
                initial.update(thinking="", signature="")
            elif part["type"] == "tool_use":
                initial["input"] = {}
            else:
                initial["text"] = ""
            body += event("content_block_start", {"type": "content_block_start", "index": index, "content_block": initial})
            if part["type"] == "thinking":
                deltas = [{"type": "thinking_delta", "thinking": reasoning}, {"type": "signature_delta", "signature": signature}]
            elif part["type"] == "tool_use":
                deltas = [{"type": "input_json_delta", "partial_json": json.dumps(call["arguments"])}]
            else:
                deltas = [{"type": "text_delta", "text": text}]
            for delta in deltas:
                body += event("content_block_delta", {"type": "content_block_delta", "index": index, "delta": delta})
            body += event("content_block_stop", {"type": "content_block_stop", "index": index})
        body += event("message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None}, "usage": {"output_tokens": 3}})
        body += event("message_stop", {"type": "message_stop"})
        return body, "text/event-stream"
    parts = []
    if thinking:
        parts.append({"text": reasoning, "thought": True, "thoughtSignature": signature})
    parts.append({"functionCall": {"id": call["id"], "name": call["name"], "args": call["arguments"]}} if call else {"text": text})
    full = {"responseId": "gemini-pi-matrix", "modelVersion": model,
            "candidates": [{"index": 0, "content": {"role": "model", "parts": parts}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 41, "candidatesTokenCount": 3, "totalTokenCount": 44}}
    return (data(full), "text/event-stream") if stream else (json.dumps(full).encode(), "application/json")
