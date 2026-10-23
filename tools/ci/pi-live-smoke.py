#!/usr/bin/env python3
"""Exercise the pinned Pi client against a fresh preview without rewriting requests."""

import http.client
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import urllib.error
import urllib.request
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

seed_spec = importlib.util.spec_from_file_location("opencode_live_smoke", Path(__file__).with_name("opencode-live-smoke.py"))
seed_module = importlib.util.module_from_spec(seed_spec)
seed_spec.loader.exec_module(seed_module)

BASE = seed_module.BASE
MODEL = seed_module.MODEL
TRACE_DIR = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "pi-live-smoke-traces"


class CaptureProxy(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_POST(self):
        request_body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        trace = TRACE_DIR / f"{self.server.case_name}.jsonl"
        responses = self.server.responses
        identity = uuid.uuid4().hex
        request_path = TRACE_DIR / f"{self.server.case_name}-{identity}.request.bin"
        response_path = TRACE_DIR / f"{self.server.case_name}-{identity}.response.bin"
        request_path.write_bytes(request_body)
        auth_headers = {"authorization", "x-api-key", "x-goog-api-key"}
        headers = {k: v for k, v in self.headers.items()
                   if k.lower() not in auth_headers | {"host", "content-length", "connection", "transfer-encoding"}}
        auth = "x-api-key" if self.headers.get("x-api-key") else "x-goog-api-key" if self.headers.get("x-goog-api-key") else "Authorization"
        headers[auth] = self.server.preview_key if auth != "Authorization" else f"Bearer {self.server.preview_key}"
        # Only transport authentication is replaced; request body bytes are untouched.
        event = {"direction": "client_request", "method": "POST", "path": self.path,
                 "headers": {k: v for k, v in self.headers.items() if k.lower() not in auth_headers},
                 "body_file": request_path.name}
        append_trace(trace, event)
        upstream = urllib.request.Request(BASE + self.path, request_body, headers, method="POST")
        response = None
        try:
            try:
                response = urllib.request.urlopen(upstream, timeout=180)
            except urllib.error.HTTPError as error:
                response = error
            self.send_response(response.status)
            content_type = response.headers.get("Content-Type", "application/json")
            for name, value in response.headers.items():
                if name.lower() not in {"connection", "keep-alive", "transfer-encoding", "content-length"}:
                    self.send_header(name, value)
            self.send_header("Connection", "close")
            self.end_headers()
            request_id = response.headers.get("x-request-id") or response.headers.get("request-id")
            responses.append({"status": response.status, "request_id": request_id})
            append_trace(trace, {"direction": "client_response", "status": response.status,
                                 "request_id": request_id,
                                 "headers": {k: v for k, v in response.headers.items() if k.lower() != "set-cookie"},
                                 "body_file": response_path.name})
            with response_path.open("wb") as capture:
                client_connected = True
                while chunk := response.read1(8192):
                    capture.write(chunk)
                    capture.flush()
                    if client_connected:
                        try:
                            self.wfile.write(chunk)
                            self.wfile.flush()
                        except OSError:
                            client_connected = False
        except (OSError, http.client.HTTPException, urllib.error.URLError) as error:
            append_trace(trace, {"direction": "transport_error", "kind": type(error).__name__})
            try:
                self.send_error(502)
            except OSError:
                pass
        finally:
            if response is not None:
                response.close()
            self.close_connection = True


def append_trace(path, event):
    with path.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(event, ensure_ascii=False) + "\n")


def pi_turn(node, cli, profile, case_name, session_id, thinking, index, prompt, expected, timeout=180):
    proxy = proxy_server
    proxy.case_name = f"{case_name}-turn-{index}"
    proxy.responses = []
    command = [node, str(cli), "--print", "--mode", "json", "--session-dir", str(profile / "sessions"),
               "--session-id", session_id, "--no-extensions", "--no-mcp", "--no-context-files",
               "--no-skills", "--no-prompt-templates", "--no-themes", "--tools", "read",
               "--model", f"anyflows/{MODEL}", "--thinking", thinking, "--", prompt]
    environment = {**os.environ, "PI_CODING_AGENT_DIR": str(profile), "PI_TELEMETRY": "0", "PI_OFFLINE": "1"}
    try:
        result = subprocess.run(command, cwd=TRACE_DIR, env=environment, text=True, encoding="utf-8",
                                errors="replace", stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        result = subprocess.CompletedProcess(command, 124, error.stdout or "")
    if isinstance(result.stdout, bytes):
        result.stdout = result.stdout.decode("utf-8", "replace")
    (TRACE_DIR / f"{case_name}-turn-{index}.pi.jsonl").write_text(result.stdout, encoding="utf-8")
    messages = []
    read_executed = False
    for line in result.stdout.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        if not isinstance(item, dict):
            continue
        if item.get("type") == "message_end":
            messages.append(item.get("message", {}))
        if item.get("type") == "tool_execution_end" and item.get("toolName") == "read" and not item.get("isError", False):
            read_executed = True
    final = next((m for m in reversed(messages) if m.get("role") == "assistant"), {})
    text_parts = [part.get("text", "") for part in final.get("content", []) if part.get("type") == "text"]
    # Anthropic receives unsigned Chat reasoning as separate text blocks. Assert
    # the final answer block, so an earlier reasoning mention cannot pass a case.
    answer = " ".join(text_parts[-1].split()) if text_parts else ""
    passed = (result.returncode == 0 and answer == " ".join(expected)
              and final.get("stopReason") == "stop"
              and bool(proxy.responses) and all(meta["status"] == 200 for meta in proxy.responses)
              and (index != 3 or read_executed))
    print(json.dumps({"case": case_name, "turn": index, "pass": passed,
                      "responses": proxy.responses}), flush=True)
    return passed


def main():
    global proxy_server
    os.umask(0o077)
    TRACE_DIR.mkdir(parents=True, exist_ok=True)
    os.chmod(TRACE_DIR, 0o700)
    profile = Path(os.environ["PI_PROFILE_DIR"])
    node = os.environ["PI_NODE"]
    cli = Path(os.environ["PI_CLI"])
    key = seed_module.seed_preview(context_window=1_000_000)
    proxy_server = ThreadingHTTPServer(("127.0.0.1", 0), CaptureProxy)
    proxy_server.preview_key = key
    proxy_server.case_name = "starting"
    proxy_server.responses = []
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
        for thinking in ["off", "medium"]:
            for protocol, api_name, suffix in [("responses", "openai-responses", "/v1"),
                                               ("chat", "openai-completions", "/v1"),
                                               ("messages", "anthropic-messages", ""),
                                               ("gemini", "google-generative-ai", "/v1beta")]:
                case_name = f"{protocol}-{thinking}"
                case_profile = profile / case_name
                case_profile.mkdir(parents=True, exist_ok=True)
                provider = models["providers"]["anyflows"]
                provider["api"] = api_name
                provider["baseUrl"] = base + suffix
                (case_profile / "models.json").write_text(json.dumps(models), encoding="utf-8")
                session_id = str(uuid.uuid4())
                prompts = [
                    ("Remember the marker PI_MEMORY_731. Reply exactly SAVED. Do not use tools.", ["SAVED"]),
                    ("What marker did I ask you to remember? Reply with only the marker. Do not use tools.", ["PI_MEMORY_731"]),
                    (f"Use the read tool to read {fixture.as_posix()}. Reply with only its contents.", ["PI_LIVE_FILE_482"]),
                    ("Reply with both tokens from this conversation, memory token first, separated by a space. Do not use tools.", ["PI_MEMORY_731", "PI_LIVE_FILE_482"]),
                ]
                for index, (prompt, expected) in enumerate(prompts, 1):
                    if not pi_turn(node, cli, case_profile, case_name, session_id, thinking, index, prompt, expected):
                        failures.append(f"{case_name}/turn-{index}")
                        break
        if failures:
            raise RuntimeError("Failed Pi cases: " + ", ".join(failures))
        print("Pi preview acceptance passed: four protocols, off/medium reasoning, four-turn memory and read-tool continuation.", flush=True)
    finally:
        proxy_server.shutdown()
        proxy_server.server_close()


if __name__ == "__main__":
    proxy_server = None
    try:
        main()
    except (AssertionError, RuntimeError, urllib.error.URLError, KeyError, TypeError) as error:
        print(f"Pi preview acceptance failed: {error}", file=sys.stderr)
        sys.exit(1)
