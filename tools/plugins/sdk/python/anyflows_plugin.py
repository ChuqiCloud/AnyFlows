"""Standard-library-only AnyFlows JSON Lines sidecar SDK (contract v1).

Protocol frames are the only bytes written to stdout. Applications should keep
their handler side-effect free and never accept credentials through public_config.
"""

from __future__ import annotations

import copy
import json
import re
import sys
import time
from collections.abc import Callable
from typing import Any, BinaryIO


class PluginError(Exception):
    """A public, bounded failure code; exception text is never sent to the host."""

    def __init__(self, code: str = "invalid_response") -> None:
        if code not in {
            "invalid_configuration", "unsupported", "authentication_required", "forbidden",
            "rate_limited", "temporarily_unavailable", "conflict", "upstream_failure",
            "invalid_response",
        }:
            raise ValueError("unsupported connector error code")
        self.code = code
        super().__init__(code)


class ProtocolError(Exception):
    """A malformed host frame ends this process instead of reusing its stream."""


class Plugin:
    def __init__(
        self,
        manifest: dict[str, Any],
        handler: Callable[[str, dict[str, Any], Any], Any],
        *,
        clock_ms: Callable[[], int] | None = None,
    ) -> None:
        self.manifest = copy.deepcopy(manifest)
        self.handler = handler
        self.clock_ms = clock_ms or (lambda: time.time_ns() // 1_000_000)
        # The wire envelope has an independent host limit. maxResponseBytes
        # limits capability values, not Manifest/health or correlation metadata.
        self.max_frame_bytes = 1_048_576
        self.max_response_bytes = manifest["limits"]["maxResponseBytes"]
        self.declarations = {item["id"]: item for item in manifest["capabilities"]}

    def respond(self, request: Any) -> dict[str, Any]:
        request = self._normalize_legacy_request(request)
        self._validate_request(request)
        response = {key: request[key] for key in (
            "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
            "deploymentGeneration", "requestId",
        )}
        try:
            if request["deadlineUnixMs"] <= self.clock_ms():
                raise PluginError("temporarily_unavailable")
            operation = request["operation"]
            kind = operation["operation"]
            if kind == "manifest":
                result = {"kind": "manifest", "value": self.manifest}
            elif kind == "health":
                result = {"kind": "health", "value": {
                    "status": "healthy", "checkedAtUnixMs": self.clock_ms(),
                }}
            else:
                declaration = self.declarations.get(operation["capabilityId"])
                if (
                    not declaration
                    or declaration["version"] != operation["capabilityVersion"]
                    or declaration["access"] != "read"
                    or declaration["sideEffect"] != "none"
                ):
                    raise PluginError("unsupported")
                output = self.handler(operation["capabilityId"], operation["publicConfig"], operation["input"])
                if self._json_size(output) > self.max_response_bytes:
                    raise PluginError("invalid_response")
                if request["deadlineUnixMs"] <= self.clock_ms():
                    raise PluginError("temporarily_unavailable")
                result = {"kind": "capability", "value": {"output": output}}
        except PluginError as error:
            result = self._error(error.code)
        except Exception:
            # Do not serialize Python exceptions, paths or handler values.
            result = self._error("upstream_failure")
        response["result"] = result
        return response

    @staticmethod
    def _error(code: str) -> dict[str, Any]:
        return {"kind": "error", "value": {"code": code, "outcome": "not_applied"}}

    @staticmethod
    def _json_size(value: Any) -> int:
        return len(json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8"))

    @staticmethod
    def _normalize_legacy_request(request: Any) -> Any:
        if not isinstance(request, dict) or not isinstance(request.get("operation"), dict):
            return request
        operation = request["operation"].copy()
        for legacy, current in (
            ("connection_id", "connectionId"), ("public_config", "publicConfig"),
            ("capability_id", "capabilityId"), ("capability_version", "capabilityVersion"),
        ):
            if legacy in operation:
                if current in operation:
                    raise ProtocolError("duplicate capability field")
                operation[current] = operation.pop(legacy)
        return {**request, "operation": operation}

    def _validate_request(self, request: Any) -> None:
        keys = {
            "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
            "deploymentGeneration", "requestId", "deadlineUnixMs", "operation",
        }
        if not isinstance(request, dict) or set(request) != keys:
            raise ProtocolError("invalid envelope")
        if (
            type(request["protocolVersion"]) is not int or request["protocolVersion"] != 1
            or request["pluginId"] != self.manifest["pluginId"]
            or request["pluginVersion"] != self.manifest["pluginVersion"]
            or not isinstance(request["artifactSha256"], str)
            or re.fullmatch(r"[a-f0-9]{64}", request["artifactSha256"]) is None
            or type(request["deploymentGeneration"]) is not int or request["deploymentGeneration"] < 1
            or not isinstance(request["requestId"], str)
            or not 1 <= len(request["requestId"].encode("utf-8")) <= 128
            or request["requestId"].strip() != request["requestId"]
            or type(request["deadlineUnixMs"]) is not int or request["deadlineUnixMs"] < 1
        ):
            raise ProtocolError("invalid correlation metadata")
        operation = request["operation"]
        if not isinstance(operation, dict) or operation.get("operation") not in {"manifest", "health", "invoke_capability"}:
            raise ProtocolError("invalid operation")
        if operation["operation"] in {"manifest", "health"}:
            if set(operation) != {"operation"}:
                raise ProtocolError("unexpected operation fields")
            return
        if set(operation) != {"operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "input"}:
            raise ProtocolError("invalid capability operation")
        if (
            not isinstance(operation["connectionId"], str)
            or not 1 <= len(operation["connectionId"].encode("utf-8")) <= 128
            or not isinstance(operation["publicConfig"], dict)
            or not isinstance(operation["capabilityId"], str)
            or type(operation["capabilityVersion"]) is not int
            or not 1 <= operation["capabilityVersion"] <= 65535
        ):
            raise ProtocolError("invalid capability fields")
        if self._json_size(operation["input"]) > self.max_response_bytes:
            raise ProtocolError("capability input exceeds advertised limit")

    def serve(self, source: BinaryIO | None = None, target: BinaryIO | None = None) -> int:
        source = source or sys.stdin.buffer
        target = target or sys.stdout.buffer
        while frame := source.readline(self.max_frame_bytes + 1):
            if len(frame) > self.max_frame_bytes or not frame.endswith(b"\n"):
                return 1
            try:
                request = json.loads(frame.decode("utf-8"), parse_constant=self._invalid_number)
                response = self.respond(request)
                payload = json.dumps(response, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8") + b"\n"
                if len(payload) > self.max_frame_bytes:
                    response["result"] = self._error("invalid_response")
                    payload = json.dumps(response, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n"
                if len(payload) > self.max_frame_bytes:
                    return 1
                target.write(payload)
                target.flush()
            except (ProtocolError, UnicodeError, ValueError, TypeError, OSError):
                return 1
        return 0

    @staticmethod
    def _invalid_number(value: str) -> None:
        raise ProtocolError("non-JSON numeric value")
