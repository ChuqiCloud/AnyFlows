"""Standard-library-only AnyFlows JSON Lines sidecar SDK (contract v1).

Protocol frames are the only bytes written to stdout. Applications should keep
their handler side-effect free and never accept credentials through public_config.
"""

from __future__ import annotations

import base64
import binascii
import copy
import inspect
import json
import re
import sys
import time
from collections.abc import Callable
from typing import Any, BinaryIO

MAX_PROVIDER_SESSION_VALUE_BYTES = 256 * 1024
MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES = 128 * 1024
MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES = 16
MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES = 16 * 1024


class PluginError(Exception):
    """A public, bounded failure code; exception text is never sent to the host."""

    def __init__(self, code: str = "invalid_response", outcome: str | None = None) -> None:
        if code not in {
            "invalid_configuration", "unsupported", "authentication_required", "forbidden",
            "rate_limited", "temporarily_unavailable", "conflict", "upstream_failure",
            "partial_success", "outcome_unknown", "invalid_response",
        }:
            raise ValueError("unsupported connector error code")
        if outcome is not None and outcome not in {"not_applied", "applied", "unknown"}:
            raise ValueError("unsupported connector operation outcome")
        self.code = code
        self.outcome = outcome
        super().__init__(code)


class ProtocolError(Exception):
    """A malformed host frame ends this process instead of reusing its stream."""


class PluginStorageError(Exception):
    """A host storage failure that is not a connector operation result."""

    def __init__(self, code: str) -> None:
        if code not in {"invalid_key", "storage_unavailable"}:
            raise ValueError("unsupported plugin storage error code")
        self.code = code
        super().__init__(code)


class PluginInvocationContext:
    """Optional, bounded host callbacks for one capability invocation."""

    def __init__(
        self,
        request: dict[str, Any],
        capability_id: str,
        capability_version: int,
        max_frame_bytes: int,
        max_response_bytes: int,
        source: BinaryIO | None,
        target: BinaryIO | None,
    ) -> None:
        self._request = request
        self._capability_id = capability_id
        self._capability_version = capability_version
        self._max_frame_bytes = max_frame_bytes
        self._max_response_bytes = max_response_bytes
        self._source = source
        self._target = target
        self._proxy_count = 0
        self._storage_count = 0

    def proxy_read(
        self,
        endpoint_id: str,
        *,
        method: str = "GET",
        query: dict[str, str] | None = None,
    ) -> Any:
        """Request one host-registered GET/HEAD endpoint and return its JSON projection.

        The plugin cannot provide a URL, path, header, body, credential, or account
        identifier. The host binds this request to the current invocation scope.
        """
        if self._source is None or self._target is None:
            raise PluginError("unsupported")
        if self._proxy_count >= 4:
            raise PluginError("rate_limited")
        if not isinstance(endpoint_id, str) or len(endpoint_id) > 128 or re.fullmatch(
            r"[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$",
            endpoint_id,
        ) is None:
            raise PluginError("invalid_configuration")
        # These are host-registered endpoints, not URLs. The endpoint determines
        # the separate capability grant used for this callback; it is distinct
        # from the parent capability currently being handled.
        endpoint_capabilities = {
            "account.profile": "external.account.profile.read",
            "account.balance": "external.account.balance.read",
        }
        proxy_capability_id = endpoint_capabilities.get(endpoint_id)
        if proxy_capability_id is None:
            raise PluginError("unsupported")
        if not isinstance(method, str) or method not in {"GET", "HEAD"}:
            raise PluginError("unsupported")
        query = {} if query is None else query
        if not isinstance(query, dict) or len(query) > 32:
            raise PluginError("invalid_configuration")
        if any(
            not isinstance(key, str)
            or re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", key) is None
            or self._is_sensitive_query_key(key)
            or not isinstance(value, str)
            or len(value.encode("utf-8")) > 512
            or any(ord(character) < 0x20 for character in value)
            for key, value in query.items()
        ):
            raise PluginError("invalid_configuration")

        self._proxy_count += 1
        proxy_request_id = f"proxy-{self._proxy_count}"
        proxy_request = {
            "protocolVersion": self._request["protocolVersion"],
            "pluginId": self._request["pluginId"],
            "pluginVersion": self._request["pluginVersion"],
            "artifactSha256": self._request["artifactSha256"],
            "deploymentGeneration": self._request["deploymentGeneration"],
            "requestId": self._request["requestId"],
            "capabilityId": proxy_capability_id,
            "capabilityVersion": 1,
            "proxyRequestId": proxy_request_id,
            "request": {
                "endpointId": endpoint_id,
                "method": method,
                "query": dict(sorted(query.items())),
            },
        }
        if self._json_size(proxy_request) + 1 > self._max_frame_bytes:
            raise PluginError("invalid_response")
        self._target.write(
            json.dumps(proxy_request, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8")
            + b"\n"
        )
        self._target.flush()
        frame = self._source.readline(self._max_frame_bytes + 1)
        if len(frame) > self._max_frame_bytes or not frame.endswith(b"\n"):
            raise ProtocolError("invalid proxy continuation frame")
        try:
            response = json.loads(
                frame.decode("utf-8"),
                parse_constant=self._invalid_number,
                object_pairs_hook=self._unique_object,
            )
        except (UnicodeError, ValueError, TypeError) as error:
            raise ProtocolError("invalid proxy continuation frame") from error
        expected_keys = {
            "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
            "deploymentGeneration", "requestId", "capabilityId", "capabilityVersion",
            "proxyRequestId", "result",
        }
        if (
            not isinstance(response, dict)
            or set(response) != expected_keys
            or any(response.get(key) != proxy_request[key] for key in (
                "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
                "deploymentGeneration", "requestId", "capabilityId", "capabilityVersion",
                "proxyRequestId",
            ))
            or not isinstance(response["result"], dict)
        ):
            raise ProtocolError("proxy continuation correlation mismatch")
        result = response["result"]
        if set(result) != {"kind", "value"} or not isinstance(result["value"], dict):
            raise ProtocolError("invalid proxy continuation result")
        if result["kind"] == "error":
            code = result["value"].get("code")
            if set(result["value"]) != {"code"} or code not in {
                "authentication_required", "forbidden", "rate_limited",
                "temporarily_unavailable", "unsupported", "invalid_response",
            }:
                raise ProtocolError("invalid proxy continuation error")
            if code == "authentication_required":
                raise PluginError("authentication_required")
            if code == "forbidden":
                raise PluginError("forbidden")
            if code == "rate_limited":
                raise PluginError("rate_limited")
            if code == "unsupported":
                raise PluginError("unsupported")
            if code == "invalid_response":
                raise PluginError("invalid_response")
            raise PluginError("temporarily_unavailable")
        if result["kind"] != "response" or set(result["value"]) != {"statusCode", "payload"}:
            raise ProtocolError("invalid proxy continuation response")
        status_code = result["value"]["statusCode"]
        payload = result["value"]["payload"]
        if type(status_code) is not int or not 200 <= status_code <= 299:
            raise ProtocolError("invalid proxy status")
        if self._json_size(payload) > self._max_response_bytes:
            raise ProtocolError("proxy projection exceeds capability limit")
        return payload

    def storage(self, operation: dict[str, Any]) -> Any:
        """Use the plugin's host-owned namespace storage for this invocation.

        The host binds the namespace to the current plugin deployment. The
        operation contains only a relative key and a bounded JSON value; it
        never carries a table name, SQL, database handle, or another plugin ID.
        """
        if self._source is None or self._target is None:
            raise PluginError("unsupported")
        if not isinstance(operation, dict) or operation.get("operation") not in {
            "get", "put", "delete", "list"
        }:
            raise PluginError("invalid_configuration")
        self._storage_count += 1
        if self._storage_count > 16:
            raise PluginError("rate_limited")
        storage_request = {
            "protocolVersion": self._request["protocolVersion"],
            "pluginId": self._request["pluginId"],
            "pluginVersion": self._request["pluginVersion"],
            "artifactSha256": self._request["artifactSha256"],
            "deploymentGeneration": self._request["deploymentGeneration"],
            "requestId": self._request["requestId"],
            "storageRequestId": f"storage-{self._storage_count}",
            "operation": copy.deepcopy(operation),
        }
        if self._json_size(storage_request) + 1 > self._max_frame_bytes:
            raise PluginError("invalid_response")
        self._target.write(
            json.dumps(storage_request, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8")
            + b"\n"
        )
        self._target.flush()
        frame = self._source.readline(self._max_frame_bytes + 1)
        if len(frame) > self._max_frame_bytes or not frame.endswith(b"\n"):
            raise ProtocolError("invalid storage continuation frame")
        try:
            response = json.loads(
                frame.decode("utf-8"),
                parse_constant=self._invalid_number,
                object_pairs_hook=self._unique_object,
            )
        except (UnicodeError, ValueError, TypeError) as error:
            raise ProtocolError("invalid storage continuation frame") from error
        expected_keys = {
            "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
            "deploymentGeneration", "requestId", "storageRequestId", "result",
        }
        if (
            not isinstance(response, dict)
            or set(response) != expected_keys
            or any(response.get(key) != storage_request[key] for key in (
                "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
                "deploymentGeneration", "requestId", "storageRequestId",
            ))
            or not isinstance(response["result"], dict)
            or set(response["result"]) != {"kind", "value"}
        ):
            raise ProtocolError("storage continuation correlation mismatch")
        result = response["result"]
        if result["kind"] == "error":
            value = result["value"]
            if not isinstance(value, dict) or set(value) != {"code"} or not isinstance(value.get("code"), str):
                raise ProtocolError("invalid storage continuation error")
            raise PluginStorageError(value["code"])
        if result["kind"] not in {"value", "entries"}:
            raise ProtocolError("invalid storage continuation result")
        if self._json_size(result["value"]) > self._max_response_bytes:
            raise ProtocolError("storage result exceeds capability limit")
        return result["value"]

    @staticmethod
    def _json_size(value: Any) -> int:
        return len(json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8"))

    @staticmethod
    def _is_sensitive_query_key(value: str) -> bool:
        characters = list(value)
        normalized: list[str] = []
        for index, character in enumerate(characters):
            previous = characters[index - 1] if index else ""
            following = characters[index + 1] if index + 1 < len(characters) else ""
            camel_boundary = character.isascii() and character.isupper() and (
                previous.isascii() and previous.islower()
                or previous.isascii() and previous.isupper() and following.isascii() and following.islower()
            )
            if camel_boundary:
                normalized.append("_")
            normalized.append(character.lower() if character.isascii() and character.isalnum() else "_")
        normalized_key = "".join(normalized)
        compact = re.sub(r"[_\-.]", "", normalized_key)
        blocked = {
            "authorization", "auth", "code", "cookie", "credential", "key",
            "password", "secret", "signature", "token",
        }
        return "apikey" in compact or any(part in blocked for part in re.split(r"[_\-.]+", normalized_key))

    @staticmethod
    def _invalid_number(value: str) -> None:
        raise ProtocolError("non-JSON numeric value")

    @staticmethod
    def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        output: dict[str, Any] = {}
        for key, value in pairs:
            if key in output:
                raise ProtocolError("duplicate JSON object key")
            output[key] = value
        return output

class Plugin:
    def __init__(
        self,
        manifest: dict[str, Any],
        handler: Callable[..., Any],
        *,
        task_handler: Callable[[str, dict[str, Any], dict[str, Any]], Any] | None = None,
        provider_handler: Callable[[str, dict[str, Any], dict[str, Any]], Any] | None = None,
        credential_handler: Callable[[str, dict[str, Any], dict[str, Any]], Any] | None = None,
        route_handler: Callable[..., Any] | None = None,
        clock_ms: Callable[[], int] | None = None,
    ) -> None:
        self.manifest = copy.deepcopy(manifest)
        self.handler = handler
        self.task_handler = task_handler
        self.provider_handler = provider_handler
        self.credential_handler = credential_handler
        self.route_handler = route_handler
        self.clock_ms = clock_ms or (lambda: time.time_ns() // 1_000_000)
        # The wire envelope has an independent host limit. maxResponseBytes
        # limits capability values, not Manifest/health or correlation metadata.
        self.max_frame_bytes = 1_048_576
        self.max_response_bytes = manifest["limits"]["maxResponseBytes"]
        self.declarations = {item["id"]: item for item in manifest["capabilities"]}
        self._handler_context_parameter = self._find_context_parameter(handler, 3)
        self._route_handler_context_parameter = self._find_context_parameter(route_handler, 4)

    def respond(
        self,
        request: Any,
        source: BinaryIO | None = None,
        target: BinaryIO | None = None,
    ) -> dict[str, Any]:
        request = self._normalize_legacy_request(request)
        self._validate_request(request)
        response = {key: request[key] for key in (
            "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
            "deploymentGeneration", "requestId",
        )}
        handler_started = False
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
            elif kind == "invoke_capability":
                declaration = self.declarations.get(operation["capabilityId"])
                if (
                    not declaration
                    or declaration["version"] != operation["capabilityVersion"]
                    or declaration["access"] != "read"
                    or declaration["sideEffect"] not in {"none", "external_read"}
                    or (
                        declaration["sideEffect"] == "external_read"
                        and (
                            operation["capabilityId"]
                            not in {"external.account.profile.read", "external.account.balance.read"}
                            or self._handler_context_parameter is None
                        )
                    )
                ):
                    raise PluginError("unsupported")
                handler_started = True
                output = self._invoke_handler(request, operation, source, target)
                if self._json_size(output) > self.max_response_bytes:
                    raise PluginError("invalid_response")
                if request["deadlineUnixMs"] <= self.clock_ms():
                    raise PluginError("outcome_unknown", "unknown")
                result = {"kind": "capability", "value": {"output": output}}
            elif kind == "invoke_task":
                declaration = self.declarations.get(operation["capabilityId"])
                if (
                    not declaration
                    or declaration["version"] != operation["capabilityVersion"]
                    or self.task_handler is None
                ):
                    raise PluginError("unsupported")
                if operation["task"]["kind"] == "submit" and (
                    declaration.get("access") != "write"
                    or declaration.get("sideEffect") != "external_write"
                    or "writeSemantics" not in declaration
                ):
                    raise PluginError("forbidden")
                handler_started = True
                output = self.task_handler(
                    operation["capabilityId"], operation["publicConfig"], operation["task"]
                )
                self._validate_task_result(output)
                if self._json_size(output) > self.max_response_bytes:
                    raise PluginError("invalid_response")
                if request["deadlineUnixMs"] <= self.clock_ms():
                    raise PluginError("outcome_unknown", "unknown")
                result = {"kind": "task", "value": output}
            elif kind == "invoke_credential":
                declaration = self.declarations.get(operation["capabilityId"])
                if (
                    operation["capabilityId"] != "credential.lifecycle"
                    or not declaration
                    or declaration["version"] != operation["capabilityVersion"]
                    or self.credential_handler is None
                ):
                    raise PluginError("unsupported")
                handler_started = True
                output = self.credential_handler(
                    operation["capabilityId"], operation["publicConfig"], operation["credential"]
                )
                self._validate_credential_result(output)
                if self._json_size(output) > self.max_response_bytes:
                    raise PluginError("invalid_response")
                if request["deadlineUnixMs"] <= self.clock_ms():
                    raise PluginError("outcome_unknown", "unknown")
                result = {"kind": "credential", "value": output}
            elif kind == "plugin_route":
                declaration = self.declarations.get(operation["capabilityId"])
                route = next(
                    (item for item in self.manifest.get("routes", [])
                     if item.get("id") == operation["routeId"]),
                    None,
                )
                if (
                    not declaration
                    or route is None
                    or route.get("capabilityId") != operation["capabilityId"]
                    or route.get("capabilityVersion") != operation["capabilityVersion"]
                    or self.route_handler is None
                ):
                    raise PluginError("unsupported")
                handler_started = True
                output = self._invoke_route_handler(request, operation, source, target)
                if self._json_size(output) > self.max_response_bytes:
                    raise PluginError("invalid_response")
                if request["deadlineUnixMs"] <= self.clock_ms():
                    raise PluginError("outcome_unknown", "unknown")
                result = {"kind": "capability", "value": {"output": output}}
            elif kind == "invoke_provider":
                declaration = self.declarations.get(operation["capabilityId"])
                if (
                    not declaration
                    or declaration["version"] != operation["capabilityVersion"]
                    or declaration.get("access") != "read"
                    or declaration.get("sideEffect") not in {"none", "external_read"}
                    or self.provider_handler is None
                ):
                    raise PluginError("unsupported")
                handler_started = True
                output = self.provider_handler(
                    operation["capabilityId"], operation["publicConfig"], operation["provider"]
                )
                self._validate_provider_result(output)
                if self._json_size(output) > self.max_response_bytes:
                    raise PluginError("invalid_response")
                if request["deadlineUnixMs"] <= self.clock_ms():
                    raise PluginError("outcome_unknown", "unknown")
                result = {"kind": "provider", "value": output}
        except ProtocolError:
            raise
        except PluginError as error:
            outcome = error.outcome or ("unknown" if handler_started else "not_applied")
            result = self._error(error.code, outcome)
        except Exception:
            # Do not serialize Python exceptions, paths or handler values.
            result = self._error(
                "upstream_failure", "unknown" if handler_started else "not_applied"
            )
        response["result"] = result
        return response

    def _invoke_handler(
        self,
        request: dict[str, Any],
        operation: dict[str, Any],
        source: BinaryIO | None,
        target: BinaryIO | None,
    ) -> Any:
        context = PluginInvocationContext(
            request,
            operation["capabilityId"],
            operation["capabilityVersion"],
            self.max_frame_bytes,
            self.max_response_bytes,
            source,
            target,
        )
        if self._handler_context_parameter is None:
            return self.handler(operation["capabilityId"], operation["publicConfig"], operation["input"])
        if self._handler_context_parameter == "keyword":
            return self.handler(
                operation["capabilityId"],
                operation["publicConfig"],
                operation["input"],
                context=context,
            )
        return self.handler(operation["capabilityId"], operation["publicConfig"], operation["input"], context)

    def _invoke_route_handler(
        self,
        request: dict[str, Any],
        operation: dict[str, Any],
        source: BinaryIO | None,
        target: BinaryIO | None,
    ) -> Any:
        if self.route_handler is None:
            raise PluginError("unsupported")
        context = PluginInvocationContext(
            request,
            operation["capabilityId"],
            operation["capabilityVersion"],
            self.max_frame_bytes,
            self.max_response_bytes,
            source,
            target,
        )
        if self._route_handler_context_parameter is None:
            return self.route_handler(
                operation["routeId"],
                operation["capabilityId"],
                operation["publicConfig"],
                operation["input"],
            )
        if self._route_handler_context_parameter == "keyword":
            return self.route_handler(
                operation["routeId"],
                operation["capabilityId"],
                operation["publicConfig"],
                operation["input"],
                context=context,
            )
        return self.route_handler(
            operation["routeId"],
            operation["capabilityId"],
            operation["publicConfig"],
            operation["input"],
            context,
        )

    @staticmethod
    def _find_context_parameter(
        handler: Callable[..., Any] | None,
        base_positional_count: int,
    ) -> str | None:
        if handler is None:
            return None
        try:
            parameters = inspect.signature(handler).parameters
        except (TypeError, ValueError):
            return None
        context_parameter = parameters.get("context")
        if context_parameter is not None and context_parameter.kind == inspect.Parameter.KEYWORD_ONLY:
            return "keyword"
        positional_count = len(
            [parameter for parameter in parameters.values() if parameter.kind in (
                inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD
            )]
        )
        return "positional" if positional_count > base_positional_count else None

    @staticmethod
    def _error(code: str, outcome: str) -> dict[str, Any]:
        return {"kind": "error", "value": {"code": code, "outcome": outcome}}

    @staticmethod
    def _json_size(value: Any) -> int:
        return len(json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8"))

    @staticmethod
    def _validate_task_result(value: Any) -> None:
        if not isinstance(value, dict) or set(value) != {"kind", "value"}:
            raise PluginError("invalid_response")

        if value["kind"] not in {"submitted", "status", "result"} or not isinstance(value["value"], dict):
            raise PluginError("invalid_response")
        payload = value["value"]
        if value["kind"] == "submitted":
            if set(payload) - {"task_id", "state", "progress_basis_points"} or not isinstance(payload.get("task_id"), str) or not 1 <= len(payload["task_id"].encode("utf-8")) <= 512 or not payload["task_id"].strip() or payload["task_id"].strip() != payload["task_id"] or any(character.isspace() or ord(character) < 32 for character in payload["task_id"]):
                raise PluginError("invalid_response")
            if payload.get("state") not in {"submitted", "queued", "running"}:
                raise PluginError("invalid_response")
            progress = payload.get("progress_basis_points")
            if progress is not None and (type(progress) is not int or not 0 <= progress <= 10000):
                raise PluginError("invalid_response")
        elif value["kind"] == "status":
            if set(payload) - {"state", "progress_basis_points", "failure_code", "retryable", "retry_after_seconds"} or payload.get("state") not in {
                "submitted", "queued", "running", "succeeded", "failed", "cancelled"
            }:
                raise PluginError("invalid_response")
            progress = payload.get("progress_basis_points")
            if progress is not None and (type(progress) is not int or not 0 <= progress <= 10000):
                raise PluginError("invalid_response")
            if payload.get("state") == "failed":
                code = payload.get("failure_code")
                if not isinstance(code, str) or not 1 <= len(code.encode("utf-8")) <= 128 or code.strip() != code or any(character.isspace() or ord(character) < 32 for character in code):
                    raise PluginError("invalid_response")
            elif "failure_code" in payload:
                raise PluginError("invalid_response")
            if payload.get("state") != "failed" and ("retryable" in payload or "retry_after_seconds" in payload):
                raise PluginError("invalid_response")
            if "retry_after_seconds" in payload and (payload.get("retryable") is not True or type(payload["retry_after_seconds"]) is not int or not 0 <= payload["retry_after_seconds"] <= 86400):
                raise PluginError("invalid_response")
        elif set(payload) - {"task_id", "output", "files", "report", "usage"} or not isinstance(payload.get("task_id"), str) or not 1 <= len(payload["task_id"].encode("utf-8")) <= 512 or not payload["task_id"].strip() or payload["task_id"].strip() != payload["task_id"] or any(character.isspace() or ord(character) < 32 for character in payload["task_id"]) or "output" not in payload or payload["output"] is None:
            raise PluginError("invalid_response")

    @staticmethod
    def _validate_provider_result(value: Any) -> None:
        if not isinstance(value, dict) or set(value) - {"kind", "models", "next_cursor", "method", "path", "endpoint_id", "headers", "body", "response", "class", "code", "retryable", "delay_ms", "reason_code", "session_id", "state", "action"}:
            raise PluginError("invalid_response")
        kind = value.get("kind")
        if kind == "models":
            models = value.get("models")
            if not isinstance(models, list) or len(models) > 500:
                raise PluginError("invalid_response")
            for model in models:
                if not isinstance(model, dict) or not isinstance(model.get("id"), str) or not 1 <= len(model["id"].encode("utf-8")) <= 128:
                    raise PluginError("invalid_response")
        elif kind == "request":
            endpoint_id = value.get("endpoint_id")
            method = value.get("method")
            if endpoint_id is not None and (not isinstance(endpoint_id, str) or not re.fullmatch(r"[a-z][a-z0-9_.-]{0,63}", endpoint_id)):
                raise PluginError("invalid_response")
            path = value.get("path")
            path_only = path.split("?", 1)[0] if isinstance(path, str) else ""
            if (
                not isinstance(method, str)
                or method not in {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
                or not isinstance(path, str)
                or not path_only.startswith("/")
                or path_only.startswith("//")
                or len(path.encode("utf-8")) > 2048
                or any(ord(char) < 32 or ord(char) == 127 or char in "#\\%" for char in path_only)
                or any(segment in {".", ".."} for segment in path_only.split("/"))
                or value.get("body") is None
            ):
                raise PluginError("invalid_response")
            if not isinstance(value.get("headers", {}), dict) or len(value.get("headers", {})) > 64:
                raise PluginError("invalid_response")
            for name, header_value in value.get("headers", {}).items():
                if (
                    not isinstance(name, str)
                    or not name
                    or len(name) > 128
                    or not re.fullmatch(r"[!#$%&'*+.^_`|~0-9A-Za-z-]+", name)
                    or not isinstance(header_value, str)
                    or len(header_value) > 8192
                    or not header_value.isascii()
                    or any(ord(char) < 32 or ord(char) == 127 for char in header_value)
                    or (endpoint_id is None and name.lower() in {"authorization", "cookie"})
                ):
                    raise PluginError("invalid_response")
        elif kind == "session":
            if (
                not isinstance(value.get("session_id"), str)
                or not value["session_id"].strip()
                or value["session_id"].strip() != value["session_id"]
                or len(value["session_id"].encode("utf-8")) > 128
                or any(ord(character) < 32 for character in value["session_id"])
                or value.get("state") is None
                or not isinstance(value.get("action"), dict)
                or Plugin._json_size(value.get("state")) > MAX_PROVIDER_SESSION_VALUE_BYTES
            ):
                raise PluginError("invalid_response")
            action = value["action"]
            action_kind = action.get("kind")
            action_fields = {
                "request": {"kind", "method", "path", "endpoint_id", "headers", "body", "stream"},
                "web_socket_connect": {"kind", "endpoint_id", "path", "headers"},
                "web_socket_send_text": {"kind", "text"},
                "web_socket_send_binary": {"kind", "data_base64"},
                "web_socket_receive": {"kind", "timeout_ms"},
                "web_socket_receive_batch": {"kind", "timeout_ms", "max_messages"},
                "web_socket_authenticate": {"kind", "request"},
                "web_socket_close": {"kind"},
                "wait": {"kind", "delay_ms"},
                "complete": {"kind", "response"},
                "abort": {"kind", "code"},
            }
            required_fields = {
                "request": {"kind", "method", "path", "body"},
                "web_socket_connect": {"kind", "endpoint_id", "path"},
                "web_socket_send_text": {"kind", "text"},
                "web_socket_send_binary": {"kind", "data_base64"},
                "web_socket_receive": {"kind", "timeout_ms"},
                "web_socket_receive_batch": {"kind", "timeout_ms", "max_messages"},
                "web_socket_authenticate": {"kind", "request"},
                "web_socket_close": {"kind"},
                "wait": {"kind", "delay_ms"},
                "complete": {"kind", "response"},
                "abort": {"kind", "code"},
            }
            if (
                not isinstance(action_kind, str)
                or action_kind not in action_fields
                or set(action) - action_fields[action_kind]
                or not required_fields[action_kind].issubset(action)
                or Plugin._json_size(action) > MAX_PROVIDER_SESSION_VALUE_BYTES
            ):
                raise PluginError("invalid_response")
            if action_kind == "request":
                endpoint_id = action.get("endpoint_id")
                path = action.get("path")
                path_only = path.split("?", 1)[0] if isinstance(path, str) else ""
                if (
                    not isinstance(action.get("method"), str)
                    or action["method"] not in {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"}
                    or not isinstance(path, str)
                    or not path_only.startswith("/")
                    or path_only.startswith("//")
                    or len(path.encode("utf-8")) > 2048
                    or any(ord(char) < 32 or ord(char) == 127 or char in "#\\%" for char in path_only)
                    or any(segment in {".", ".."} for segment in path_only.split("/"))
                    or (endpoint_id is not None and (not isinstance(endpoint_id, str) or not re.fullmatch(r"[a-z][a-z0-9_.-]{0,63}", endpoint_id)))
                    or action.get("body") is None
                    or action.get("stream", False) is not False
                    or not isinstance(action.get("headers", {}), dict)
                    or len(action.get("headers", {})) > 64
                ):
                    raise PluginError("invalid_response")
            elif action_kind == "web_socket_connect":
                endpoint_id = action.get("endpoint_id")
                path = action.get("path")
                if (
                    not isinstance(endpoint_id, str)
                    or not re.fullmatch(r"[a-z][a-z0-9_.-]{0,63}", endpoint_id)
                    or not isinstance(path, str)
                    or not path.startswith("/")
                    or path.startswith("//")
                    or len(path.encode("utf-8")) > 2048
                    or any(ord(char) < 32 or ord(char) == 127 or char in "#\\%" for char in path.split("?", 1)[0])
                    or any(segment in {".", ".."} for segment in path.split("?", 1)[0].split("/"))
                    or not isinstance(action.get("headers", {}), dict)
                    or len(action.get("headers", {})) > 64
                ):
                    raise PluginError("invalid_response")
            elif action_kind == "web_socket_send_text":
                text = action.get("text")
                if (
                    not isinstance(text, str)
                    or len(text.encode("utf-8")) > MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES
                ):
                    raise PluginError("invalid_response")
            elif action_kind == "web_socket_send_binary":
                encoded = action.get("data_base64")
                if not isinstance(encoded, str) or len(encoded) > ((MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES + 2) // 3) * 4:
                    raise PluginError("invalid_response")
                try:
                    decoded = base64.b64decode(encoded, validate=True)
                except (binascii.Error, ValueError, TypeError):
                    raise PluginError("invalid_response") from None
                if len(decoded) > MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES:
                    raise PluginError("invalid_response")
            elif action_kind == "web_socket_receive":
                if type(action.get("timeout_ms")) is not int or not 1 <= action["timeout_ms"] <= 30_000:
                    raise PluginError("invalid_response")
            elif action_kind == "web_socket_receive_batch":
                if (
                    type(action.get("timeout_ms")) is not int
                    or not 1 <= action["timeout_ms"] <= 30_000
                    or type(action.get("max_messages")) is not int
                    or not 1 <= action["max_messages"] <= MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES
                ):
                    raise PluginError("invalid_response")
            elif action_kind == "web_socket_authenticate":
                auth_request = action.get("request")
                if (
                    not isinstance(auth_request, dict)
                    or auth_request.get("challenge") is None
                    or Plugin._json_size(auth_request) > 16 * 1024
                ):
                    raise PluginError("invalid_response")
            elif action_kind == "web_socket_close":
                pass
            elif action_kind == "wait":
                if type(action.get("delay_ms")) is not int or not 0 <= action["delay_ms"] <= 30_000:
                    raise PluginError("invalid_response")
            elif action_kind == "complete":
                if action.get("response") is None:
                    raise PluginError("invalid_response")
            elif action_kind == "abort":
                if not isinstance(action.get("code"), str) or not action["code"].strip():
                    raise PluginError("invalid_response")
            else:
                raise PluginError("invalid_response")
        elif kind == "response":
            if value.get("response") is None:
                raise PluginError("invalid_response")
        elif kind == "failure":
            if not isinstance(value.get("class"), str) or not isinstance(value.get("code"), str) or not value["code"].strip():
                raise PluginError("invalid_response")
        elif kind == "retry":
            if type(value.get("retryable")) is not bool or type(value.get("delay_ms")) is not int or not 0 <= value["delay_ms"] <= 86400000 or not isinstance(value.get("reason_code"), str) or not value["reason_code"].strip():
                raise PluginError("invalid_response")
        else:
            raise PluginError("invalid_response")


    @staticmethod
    def _validate_credential_result(value: Any) -> None:
        if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
            raise PluginError("invalid_response")
        kind = value["kind"]
        if kind == "session":
            if set(value) != {"kind", "material", "result"} or not isinstance(value["material"], dict) or Plugin._json_size(value["material"]) > 4096 or not isinstance(value["result"], dict) or value["result"].get("kind") == "session":
                raise PluginError("invalid_response")
            Plugin._validate_credential_result(value["result"])
            return
        if kind == "login":
            if set(value) - {"kind", "authorization_url", "state", "complete"} or type(value.get("complete")) is not bool:
                raise PluginError("invalid_response")
        elif kind == "refreshed":
            if set(value) - {"kind", "expires_at_unix_ms"}:
                raise PluginError("invalid_response")
        elif kind == "revoked":
            if set(value) != {"kind"}:
                raise PluginError("invalid_response")
        elif kind == "signed":
            if set(value) - {"kind", "headers", "query", "body"} or not isinstance(value.get("headers"), dict):
                raise PluginError("invalid_response")
            if any(str(name).lower() in {"host", "cookie", "content-length", "proxy-authorization", "connection", "transfer-encoding", "upgrade"} for name in value["headers"]):
                raise PluginError("invalid_response")
        elif kind == "web_socket_auth":
            frames = value.get("frames")
            if set(value) != {"kind", "frames"} or not isinstance(frames, list) or not 1 <= len(frames) <= 8:
                raise PluginError("invalid_response")
            for frame in frames:
                if not isinstance(frame, dict):
                    raise PluginError("invalid_response")
                if frame.get("kind") == "text":
                    if set(frame) != {"kind", "text"} or not isinstance(frame.get("text"), str) or len(frame["text"].encode("utf-8")) > MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES:
                        raise PluginError("invalid_response")
                elif frame.get("kind") == "binary":
                    encoded = frame.get("data_base64")
                    if set(frame) != {"kind", "data_base64"} or not isinstance(encoded, str) or len(encoded) > ((MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES + 2) // 3) * 4:
                        raise PluginError("invalid_response")
                    try:
                        decoded = base64.b64decode(encoded, validate=True)
                    except (binascii.Error, ValueError, TypeError):
                        raise PluginError("invalid_response") from None
                    if len(decoded) > MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES:
                        raise PluginError("invalid_response")
        elif kind == "balance":
            if set(value) - {"kind", "amount", "currency", "as_of_unix_ms"} or not isinstance(value.get("amount"), str) or not isinstance(value.get("currency"), str):
                raise PluginError("invalid_response")
        else:
            raise PluginError("invalid_response")

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
        if not isinstance(operation, dict) or operation.get("operation") not in {"manifest", "health", "invoke_capability", "invoke_task", "invoke_provider", "invoke_credential", "plugin_route"}:
            raise ProtocolError("invalid operation")
        if operation["operation"] in {"manifest", "health"}:
            if set(operation) != {"operation"}:
                raise ProtocolError("unexpected operation fields")
            return
        if operation["operation"] == "invoke_task":
            if set(operation) != {"operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "task"}:
                raise ProtocolError("invalid task operation")
        elif operation["operation"] == "invoke_provider":
            if set(operation) != {"operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "provider"}:
                raise ProtocolError("invalid provider operation")
        elif operation["operation"] == "invoke_credential":
            if set(operation) != {"operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "credential"}:
                raise ProtocolError("invalid credential operation")
        elif operation["operation"] == "plugin_route":
            if set(operation) != {"operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "routeId", "input"}:
                raise ProtocolError("invalid plugin route operation")
        elif set(operation) != {"operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "input"}:
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
        if operation["operation"] == "invoke_task":
            task = operation["task"]
            if not isinstance(task, dict) or task.get("kind") not in {"submit", "status", "cancel", "result"}:
                raise ProtocolError("invalid task operation")
            if task["kind"] == "submit" and (
                set(task) - {"kind", "model", "request", "idempotency_key"}
                or not isinstance(task.get("model"), str)
                or not 1 <= len(task["model"].encode("utf-8")) <= 128
                or not task["model"].strip()
                or task["model"].strip() != task["model"]
                or any(character.isspace() or ord(character) < 32 for character in task["model"])
                or "request" not in task
            ):
                raise ProtocolError("invalid task operation")
            if "idempotency_key" in task and (
                not isinstance(task["idempotency_key"], str)
                or not 1 <= len(task["idempotency_key"].encode("utf-8")) <= 256
                or not task["idempotency_key"].strip()
                or task["idempotency_key"].strip() != task["idempotency_key"]
                or any(character.isspace() or ord(character) < 32 for character in task["idempotency_key"])
            ):
                raise ProtocolError("invalid task operation")
            if task["kind"] == "status" and (
                set(task) != {"kind", "task_id"}
                or not isinstance(task["task_id"], str)
                or not 1 <= len(task["task_id"].encode("utf-8")) <= 512
                or not task["task_id"].strip()
                or task["task_id"].strip() != task["task_id"]
                or any(character.isspace() or ord(character) < 32 for character in task["task_id"])
            ):
                raise ProtocolError("invalid task operation")
            if task["kind"] == "cancel" and (
                set(task) - {"kind", "task_id", "reason"}
                or not isinstance(task.get("task_id"), str)
                or not 1 <= len(task["task_id"].encode("utf-8")) <= 512
                or not task["task_id"].strip()
                or task["task_id"].strip() != task["task_id"]
                or any(character.isspace() or ord(character) < 32 for character in task["task_id"])
                or ("reason" in task and (not isinstance(task["reason"], str) or not 1 <= len(task["reason"].encode("utf-8")) <= 256 or task["reason"].strip() != task["reason"]))
            ):
                raise ProtocolError("invalid task operation")
            if task["kind"] == "result" and (
                set(task) != {"kind", "task_id", "response"}
                or not isinstance(task["task_id"], str)
                or not 1 <= len(task["task_id"].encode("utf-8")) <= 512
                or not task["task_id"].strip()
                or task["task_id"].strip() != task["task_id"]
                or any(character.isspace() or ord(character) < 32 for character in task["task_id"])
                or task["response"] is None
            ):
                raise ProtocolError("invalid task operation")
            if self._json_size(task) > self.max_response_bytes:
                raise ProtocolError("task input exceeds advertised limit")
            return
        if operation["operation"] == "invoke_credential":
            credential = operation["credential"]
            if (
                not isinstance(credential, dict)
                or credential.get("kind") not in {"bound", "login", "refresh", "revoke", "sign", "web_socket_auth", "balance"}
                or self._json_size(credential) > self.max_response_bytes
            ):
                raise ProtocolError("invalid credential operation")
            return
        if operation["operation"] == "invoke_provider":
            if self._json_size(operation["provider"]) > self.max_response_bytes:
                raise ProtocolError("provider input exceeds advertised limit")
            return
        if self._json_size(operation["input"]) > self.max_response_bytes:
            raise ProtocolError("capability input exceeds advertised limit")

    def serve(self, source: BinaryIO | None = None, target: BinaryIO | None = None) -> int:
        source = source or sys.stdin.buffer
        target = target or sys.stdout.buffer
        while frame := source.readline(self.max_frame_bytes + 1):
            if len(frame) > self.max_frame_bytes or not frame.endswith(b"\n"):
                return 1
            try:
                request = json.loads(
                    frame.decode("utf-8"),
                    parse_constant=self._invalid_number,
                    object_pairs_hook=self._unique_object,
                )
                response = self.respond(request, source, target)
                payload = json.dumps(response, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8") + b"\n"
                if len(payload) > self.max_frame_bytes:
                    response["result"] = self._error("invalid_response", "unknown")
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

    @staticmethod
    def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        output: dict[str, Any] = {}
        for key, value in pairs:
            if key in output:
                raise ProtocolError("duplicate JSON object key")
            output[key] = value
        return output
