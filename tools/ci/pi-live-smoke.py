#!/usr/bin/env python3
"""Exercise the pinned Pi client against a fresh preview without rewriting requests."""

import http.client
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

seed_spec = importlib.util.spec_from_file_location("opencode_live_smoke", Path(__file__).with_name("opencode-live-smoke.py"))
seed_module = importlib.util.module_from_spec(seed_spec)
seed_spec.loader.exec_module(seed_module)
fixture_spec = importlib.util.spec_from_file_location("pi_upstream_fixture", Path(__file__).with_name("pi-upstream-fixture.py"))
fixture_module = importlib.util.module_from_spec(fixture_spec)
fixture_spec.loader.exec_module(fixture_module)

BASE = seed_module.BASE
MODEL = seed_module.MODEL
TRACE_DIR = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "pi-live-smoke-traces"
DIAGNOSTIC_TEXT_LIMIT = 384
DIAGNOSTIC_BODY_LIMIT = 64 * 1024
DIAGNOSTIC_ITEMS_LIMIT = 6
CAPTURE_COMPLETION_TIMEOUT = 30


class CaptureProxy(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_POST(self):
        case_name = self.server.case_name
        trace = TRACE_DIR / f"{case_name}.jsonl"
        responses = self.server.responses
        captures = self.server.response_captures
        identity = uuid.uuid4().hex
        request_path = TRACE_DIR / f"{case_name}-{identity}.request.bin"
        response_path = TRACE_DIR / f"{case_name}-{identity}.response.bin"
        capture_state = {"status": None, "request_id": None, "content_type": "",
                         "content_length": None, "transfer_encoding": None,
                         "content_encoding": None, "connection": None,
                         "expected_content_length": None, "actual_bytes": 0,
                         "read_complete": False, "read_error": None,
                         "body_file": response_path.name, "completed": threading.Event()}
        captures.append(capture_state)
        response = None
        headers_committed = False
        try:
            request_body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            request_path.write_bytes(request_body)
            auth_headers = {"authorization", "x-api-key", "x-goog-api-key"}
            headers = {k: v for k, v in self.headers.items()
                       if k.lower() not in auth_headers | {"host", "content-length", "connection", "transfer-encoding"}}
            auth = "x-api-key" if self.headers.get("x-api-key") else "x-goog-api-key" if self.headers.get("x-goog-api-key") else "Authorization"
            headers[auth] = self.server.preview_key if auth != "Authorization" else f"Bearer {self.server.preview_key}"
            # Only transport authentication is replaced; request body bytes are untouched.
            append_trace(trace, {"direction": "client_request", "method": "POST", "path": self.path,
                                 "headers": {k: v for k, v in self.headers.items() if k.lower() not in auth_headers},
                                 "body_file": request_path.name})
            upstream = urllib.request.Request(BASE + self.path, request_body, headers, method="POST")
            try:
                response = urllib.request.urlopen(upstream, timeout=180)
            except urllib.error.HTTPError as error:
                response = error
            capture_state.update(status=response.status,
                                 request_id=response.headers.get("x-request-id") or response.headers.get("request-id"),
                                 content_type=response.headers.get("Content-Type", "application/json"),
                                 content_length=response.headers.get("Content-Length"),
                                 transfer_encoding=response.headers.get("Transfer-Encoding"),
                                 content_encoding=response.headers.get("Content-Encoding"),
                                 connection=response.headers.get("Connection"))
            declared_length = capture_state["content_length"]
            # HTTP chunked framing takes precedence over Content-Length.
            if declared_length is not None and "chunked" not in (capture_state["transfer_encoding"] or "").lower():
                if re.fullmatch(r"[0-9]+", declared_length.strip()):
                    capture_state["expected_content_length"] = int(declared_length)
                else:
                    capture_state["read_error"] = {"kind": "InvalidContentLength"}
            self.send_response(response.status)
            for name, value in response.headers.items():
                if name.lower() not in {"connection", "keep-alive", "transfer-encoding", "content-length"}:
                    self.send_header(name, value)
            self.send_header("Connection", "close")
            # Do not attempt a second HTTP response even if flushing headers fails.
            headers_committed = True
            self.end_headers()
            request_id = capture_state["request_id"]
            responses.append({"status": response.status, "request_id": request_id})
            append_trace(trace, {"direction": "client_response", "status": response.status,
                                 "request_id": request_id,
                                 "headers": {k: v for k, v in response.headers.items() if k.lower() != "set-cookie"},
                                 "body_file": response_path.name})
            with response_path.open("wb") as capture:
                client_connected = True
                while True:
                    try:
                        chunk = response.read1(8192)
                    except http.client.IncompleteRead as error:
                        if error.partial:
                            capture.write(error.partial)
                            capture.flush()
                            capture_state["actual_bytes"] += len(error.partial)
                        raise
                    if not chunk:
                        break
                    capture.write(chunk)
                    capture.flush()
                    capture_state["actual_bytes"] += len(chunk)
                    if client_connected:
                        try:
                            self.wfile.write(chunk)
                            self.wfile.flush()
                        except OSError:
                            client_connected = False
                expected_length = capture_state["expected_content_length"]
                if expected_length is not None and capture_state["actual_bytes"] != expected_length:
                    capture_state["read_error"] = {"kind": "ContentLengthMismatch",
                                                   "expected_bytes": expected_length,
                                                   "actual_bytes": capture_state["actual_bytes"]}
                capture_state["read_complete"] = capture_state["read_error"] is None
        except (OSError, http.client.HTTPException, urllib.error.URLError, ValueError) as error:
            capture_state["read_error"] = {"kind": type(error).__name__, "message": str(error)}
            append_trace(trace, {"direction": "transport_error", "kind": type(error).__name__})
            if not headers_committed:
                try:
                    self.send_error(502)
                except OSError:
                    pass
        finally:
            try:
                if response is not None:
                    response.close()
            finally:
                self.close_connection = True
                try:
                    append_trace(trace, {"direction": "capture_complete",
                                         **{name: capture_state[name] for name in
                                            ["body_file", "content_type", "content_length", "transfer_encoding",
                                             "content_encoding", "connection", "expected_content_length",
                                             "actual_bytes", "read_complete"]},
                                         "read_error": diagnostic_read_error(capture_state["read_error"],
                                                                             (self.server.preview_key,))})
                finally:
                    capture_state["completed"].set()


def append_trace(path, event):
    with path.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(event, ensure_ascii=False) + "\n")


def diagnostic_text(value, secrets=()):
    """Bound controlled error text after removing credentials, never print headers."""
    text = str(value)
    for secret in secrets:
        if secret:
            text = text.replace(secret, "[redacted]")
    text = re.sub(r"(?i)\b(?:bearer|basic)\s+[A-Za-z0-9._~+/=-]+", "[redacted authorization]", text)
    text = re.sub(r'''(?ix)
        ["']?(?:authorization|proxy-authorization|x-api-key|x-goog-api-key|api[_-]?key|
        access[_-]?token|refresh[_-]?token|password|secret)["']?\s*[:=]\s*
        (?:"[^"]*"|'[^']*'|[^\s,;}\]]+)
    ''', "[redacted credential]", text)
    text = re.sub(r"(?i)(https?://)[^/\s:@]+:[^/\s@]+@", r"\1[redacted]@", text)
    text = re.sub(r"(?i)([?&](?:key|token|api_key|access_token)=)[^&\s]+", r"\1[redacted]", text)
    text = re.sub(r"\bsk-[A-Za-z0-9_-]+", "[redacted key]", text)
    text = " ".join(text.split())
    return text[:DIAGNOSTIC_TEXT_LIMIT] + ("…" if len(text) > DIAGNOSTIC_TEXT_LIMIT else "")


def error_summary(value, secrets=(), depth=0):
    """Select error fields only; arbitrary response/request objects stay private."""
    if depth >= 3:
        return None
    if isinstance(value, str):
        return diagnostic_text(value, secrets)
    if isinstance(value, (bool, int, float)) or value is None:
        return value
    if not isinstance(value, dict):
        return None
    summary = {}
    for key in ["type", "code", "message", "errorMessage", "status", "stopReason",
                "reason", "error", "incomplete_details"]:
        if key in value and value[key] is not None:
            item = error_summary(value[key], secrets, depth + 1)
            if item is not None:
                summary[key] = item
    return summary or None


def diagnostic_read_error(error, secrets=()):
    if error is None:
        return None
    return {key: diagnostic_text(value, secrets) if isinstance(value, str) else value
            for key, value in error.items()
            if key in {"kind", "message", "expected_bytes", "actual_bytes"}}


def wait_for_captures(captures, timeout=CAPTURE_COMPLETION_TIMEOUT):
    """Allow proxy workers to finish before reading their capture files."""
    deadline = time.monotonic() + timeout
    index = 0
    while index < len(captures):
        completed = captures[index]["completed"]
        if not completed.wait(max(0, deadline - time.monotonic())):
            return False
        index += 1
    return True


def captured_response_diagnostics(captures, secrets=()):
    reports = []
    for capture in captures[:DIAGNOSTIC_ITEMS_LIMIT]:
        report = {"status": capture["status"], "body_file": capture["body_file"]}
        report.update({name: capture.get(name) for name in
                       ["content_type", "content_length", "transfer_encoding", "content_encoding",
                        "connection", "expected_content_length", "actual_bytes", "read_complete"]})
        completed = capture.get("completed")
        report["capture_complete"] = completed.is_set() if completed is not None else None
        report["transport_error"] = diagnostic_read_error(capture.get("read_error"), secrets)
        status = capture["status"] or 0
        path = TRACE_DIR / capture["body_file"]
        try:
            with path.open("rb") as stream:
                size = path.stat().st_size
                head = stream.read(DIAGNOSTIC_BODY_LIMIT)
                tail = b""
                if size > DIAGNOSTIC_BODY_LIMIT:
                    stream.seek(max(DIAGNOSTIC_BODY_LIMIT, size - DIAGNOSTIC_BODY_LIMIT))
                    tail = stream.read(DIAGNOSTIC_BODY_LIMIT)
            report["body_bytes"] = size
            report["body_sample_truncated"] = size > len(head) + len(tail)
            samples = [head.decode("utf-8", "replace")]
            if tail:
                samples.append(tail.decode("utf-8", "replace"))
            errors = []
            for sample in samples:
                candidates = [sample] if "event-stream" not in capture["content_type"].lower() else []
                candidates.extend(line[5:].strip() for line in sample.splitlines() if line.startswith("data:"))
                for candidate in candidates:
                    try:
                        value = json.loads(candidate)
                    except ValueError:
                        continue
                    if not isinstance(value, dict):
                        continue
                    terminal = value.get("response")
                    if isinstance(terminal, dict):
                        value = terminal
                    failed = (status >= 400 or value.get("error") is not None
                              or value.get("type") in {"error", "response.failed", "response.incomplete"}
                              or value.get("status") in {"failed", "incomplete"})
                    summary = error_summary(value, secrets) if failed else None
                    if summary and summary not in errors and len(errors) < DIAGNOSTIC_ITEMS_LIMIT:
                        errors.append(summary)
            report["errors"] = errors
            if status >= 400 and not errors:
                report["error_body_excerpt"] = diagnostic_text(samples[0], secrets)
        except OSError as error:
            report["capture_error"] = type(error).__name__
        reports.append(report)
    return {"captures": reports, "omitted_captures": max(0, len(captures) - len(reports))}


def fixture_diagnostics(server):
    case = server.active
    failures = server.failures[case["failure_start"]:]
    hits = server.hits[case["start"]:]
    secrets = (proxy_server.preview_key, "preview-capture-proxy")
    return {"expected_requests": 2 if case["turn"] == 3 else 1, "observed_requests": len(hits),
            "hits": [{key: hit[key] for key in ["target", "valid", "stream", "tool", "tool_result"] if key in hit}
                     for hit in hits[:DIAGNOSTIC_ITEMS_LIMIT]],
            "failures": [{"kind": diagnostic_text(item.get("kind", ""), secrets),
                          "message": diagnostic_text(item.get("message", ""), secrets)}
                         for item in failures[:DIAGNOSTIC_ITEMS_LIMIT]],
            "omitted_failures": max(0, len(failures) - DIAGNOSTIC_ITEMS_LIMIT)}


def pi_turn(node, cli, profile, case_name, session_id, model, thinking, index, prompt, expected, timeout=180):
    proxy = proxy_server
    proxy.case_name = f"{case_name}-turn-{index}"
    proxy.responses = []
    proxy.response_captures = []
    command = [node, str(cli), "--print", "--mode", "json", "--session-dir", str(profile / "sessions"),
               "--session-id", session_id, "--no-extensions", "--no-mcp", "--no-context-files",
               "--no-skills", "--no-prompt-templates", "--no-themes", "--tools", "read",
               "--model", f"anyflows/{model}", "--thinking", thinking, "--", prompt]
    environment = {**os.environ, "PI_CODING_AGENT_DIR": str(profile), "PI_TELEMETRY": "0", "PI_OFFLINE": "1"}
    try:
        result = subprocess.run(command, cwd=TRACE_DIR, env=environment, text=True, encoding="utf-8",
                                errors="replace", stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        result = subprocess.CompletedProcess(command, 124, error.stdout or "")
    if isinstance(result.stdout, bytes):
        result.stdout = result.stdout.decode("utf-8", "replace")
    captures_completed = wait_for_captures(proxy.response_captures)
    (TRACE_DIR / f"{case_name}-turn-{index}.pi.jsonl").write_text(result.stdout, encoding="utf-8")
    messages = []
    read_executed = False
    pi_errors = []
    secrets = (proxy.preview_key, "preview-capture-proxy")
    for line in result.stdout.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        if not isinstance(item, dict):
            continue
        if item.get("type") == "message_end":
            messages.append(item.get("message", {}))
            message = item.get("message", {})
            if isinstance(message, dict) and (message.get("errorMessage") or message.get("stopReason") in {"error", "aborted"}):
                summary = error_summary(message, secrets)
                if summary and len(pi_errors) < DIAGNOSTIC_ITEMS_LIMIT:
                    pi_errors.append(summary)
        if item.get("type") == "error" or item.get("error") is not None:
            summary = error_summary(item, secrets)
            if summary and len(pi_errors) < DIAGNOSTIC_ITEMS_LIMIT:
                pi_errors.append(summary)
        if item.get("type") == "tool_execution_end" and item.get("toolName") == "read" and not item.get("isError", False):
            read_executed = True
    final = next((m for m in reversed(messages) if m.get("role") == "assistant"), {})
    text_parts = [part.get("text", "") for part in final.get("content", []) if part.get("type") == "text"]
    # Inspect every visible text block. Foreign reasoning may be a separate
    # replayable text block for Messages, but cannot substitute for the answer.
    answer = " ".join(" ".join(text_parts).split())
    expected_text = " ".join(expected)
    recognized_answers = {expected_text, "Controlled fixture reasoning. " + expected_text}
    usage = final.get("usage", {})
    usage_present = isinstance(usage, dict) and any(isinstance(usage.get(name), (int, float)) and usage[name] > 0
                                                   for name in ["input", "output", "totalTokens"])
    passed = (result.returncode == 0 and answer in recognized_answers
              and final.get("stopReason") == "stop"
              and usage_present
              and captures_completed and bool(proxy.response_captures)
              and all(capture["read_complete"] for capture in proxy.response_captures)
              and bool(proxy.responses) and all(meta["status"] == 200 for meta in proxy.responses)
              and (index != 3 or read_executed))
    report = {"case": case_name, "turn": index, "pass": passed, "responses": proxy.responses}
    if not passed:
        report["diagnostics"] = {"exit_code": result.returncode,
                                 "captures_completed": captures_completed,
                                 "stop_reason": diagnostic_text(final.get("stopReason"), secrets),
                                 "assistant_messages": sum(m.get("role") == "assistant" for m in messages),
                                 "answer_matches": answer in recognized_answers,
                                 "answer_chars": len(answer),
                                 "answer_excerpt": diagnostic_text(answer, secrets),
                                 "usage_present": usage_present, "read_executed": read_executed,
                                 "pi_errors": pi_errors,
                                 "responses": captured_response_diagnostics(proxy.response_captures, secrets)}
    print(json.dumps(report), flush=True)
    return passed


def main():
    global proxy_server
    os.umask(0o077)
    TRACE_DIR.mkdir(parents=True, exist_ok=True)
    os.chmod(TRACE_DIR, 0o700)
    try:
        repetitions = int(os.environ.get("PI_MATRIX_REPETITIONS", "1"))
    except ValueError as error:
        raise RuntimeError("PI_MATRIX_REPETITIONS must be a positive integer") from error
    if not 1 <= repetitions <= 5:
        raise RuntimeError("PI_MATRIX_REPETITIONS must be between 1 and 5")
    profile = Path(os.environ["PI_PROFILE_DIR"])
    node = os.environ["PI_NODE"]
    cli = Path(os.environ["PI_CLI"])
    fixture_server = fixture_module.start_fixture(TRACE_DIR)
    key = fixture_module.seed_preview(seed_module, fixture_server)
    proxy_server = ThreadingHTTPServer(("127.0.0.1", 0), CaptureProxy)
    proxy_server.preview_key = key
    proxy_server.case_name = "starting"
    proxy_server.responses = []
    proxy_server.response_captures = []
    threading.Thread(target=proxy_server.serve_forever, daemon=True).start()
    try:
        base = f"http://127.0.0.1:{proxy_server.server_address[1]}"
        models = {"providers": {"anyflows": {"api": "openai-responses", "baseUrl": base + "/v1",
                  "apiKey": "preview-capture-proxy", "models": [{"id": MODEL, "name": MODEL,
                  "contextWindow": 1_000_000, "maxTokens": 384_000, "reasoning": True,
                  "input": ["text"], "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}}]}}}
        fixture = TRACE_DIR / "read-fixture.txt"
        fixture.write_text("PI_LIVE_FILE_482", encoding="utf-8")
        failures = []
        for repetition in range(1, repetitions + 1):
            for target, thinking in ((target, thinking) for target in fixture_module.PROTOCOLS
                                     for thinking in ["off", "medium"]):
                for protocol, api_name, suffix in [("responses", "openai-responses", "/v1"),
                                                   ("chat", "openai-completions", "/v1"),
                                                   ("messages", "anthropic-messages", ""),
                                                   ("gemini", "google-generative-ai", "/v1beta")]:
                    case_name = f"{protocol}-to-{target}-{thinking}"
                    if repetitions > 1:
                        case_name += f"-repeat-{repetition}"
                    model = fixture_module.model_for(target)
                    case_profile = profile / case_name
                    case_profile.mkdir(parents=True, exist_ok=True)
                    provider = models["providers"]["anyflows"]
                    provider["api"] = api_name
                    provider["baseUrl"] = base + suffix
                    provider["models"][0]["id"] = model
                    provider["models"][0]["name"] = model
                    (case_profile / "models.json").write_text(json.dumps(models), encoding="utf-8")
                    session_id = str(uuid.uuid4())
                    prompts = [
                        ("Remember the marker PI_MEMORY_731. Reply exactly SAVED. Do not use tools.", ["SAVED"]),
                        ("What marker did I ask you to remember? Reply with only the marker. Do not use tools.", ["PI_MEMORY_731"]),
                        (f"Use the read tool to read {fixture.as_posix()}. Reply with only its contents.", ["PI_LIVE_FILE_482"]),
                        ("Reply with both tokens from this conversation, memory token first, separated by a space. Do not use tools.", ["PI_MEMORY_731", "PI_LIVE_FILE_482"]),
                    ]
                    for index, (prompt, expected) in enumerate(prompts, 1):
                        fixture_module.begin_turn(fixture_server, protocol, target, thinking, index, fixture)
                        passed = pi_turn(node, cli, case_profile, case_name, session_id, model, thinking, index, prompt, expected)
                        try:
                            fixture_module.verify_turn(fixture_server)
                        except AssertionError as error:
                            print(json.dumps({"case":case_name,"turn":index,"fixture_pass":False,
                                              "reason":diagnostic_text(error, (key,)),
                                              "fixture_diagnostics":fixture_diagnostics(fixture_server)}), flush=True)
                            passed = False
                        if not passed:
                            failures.append(f"{case_name}/turn-{index}")
                            break
        if failures:
            raise RuntimeError("Failed Pi cases: " + ", ".join(failures))
        print(f"Pi controlled acceptance passed: 16 protocol routes, {repetitions} repetition(s), off/medium reasoning, four-turn memory, tool execution and complete output replay; no paid upstream requests.", flush=True)
    finally:
        proxy_server.shutdown()
        proxy_server.server_close()
        fixture_server.shutdown()
        fixture_server.server_close()


if __name__ == "__main__":
    proxy_server = None
    try:
        main()
    except (AssertionError, RuntimeError, urllib.error.URLError, KeyError, TypeError) as error:
        print(f"Pi preview acceptance failed: {error}", file=sys.stderr)
        sys.exit(1)
