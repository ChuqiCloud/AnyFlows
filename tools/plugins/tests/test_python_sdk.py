"""Wire and failure-boundary regressions for the reusable Python sidecar SDK."""

from __future__ import annotations

import copy
import importlib.util
import io
import json
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk" / "python"))
from anyflows_plugin import Plugin, PluginError, ProtocolError  # noqa: E402


MANIFEST = json.loads((ROOT / "examples" / "catalog" / "manifest.json").read_text(encoding="utf-8"))


def request(operation: dict, manifest: dict = MANIFEST) -> dict:
    return {
        "protocolVersion": 1,
        "pluginId": manifest["pluginId"],
        "pluginVersion": manifest["pluginVersion"],
        "artifactSha256": "a" * 64,
        "deploymentGeneration": 7,
        "requestId": "host-request-1",
        "deadlineUnixMs": 2000,
        "operation": operation,
    }


def external_profile_manifest() -> dict:
    manifest = copy.deepcopy(MANIFEST)
    declaration = manifest["capabilities"][0]
    declaration["id"] = "external.account.profile.read"
    declaration["sideEffect"] = "external_read"
    declaration["inputSchema"] = {"type": "object", "additionalProperties": False}
    declaration["outputSchema"] = {
        "type": "object",
        "additionalProperties": False,
        "required": ["profile"],
        "properties": {"profile": {"type": "object"}},
    }
    return manifest


def live_balance_review_manifest() -> dict:
    manifest = copy.deepcopy(MANIFEST)
    declaration = manifest["capabilities"][0]
    declaration["id"] = "account.balance.snapshot.live-review.read"
    declaration["inputSchema"] = {
        "type": "object",
        "additionalProperties": False,
        "required": ["snapshots"],
        "properties": {"snapshots": {"type": "array", "minItems": 1, "maxItems": 20}},
    }
    declaration["outputSchema"] = {
        "type": "object",
        "additionalProperties": False,
        "required": ["summary", "checks"],
        "properties": {"summary": {"type": "string"}, "checks": {"type": "array"}},
    }
    return manifest


def plugin_route_manifest() -> dict:
    manifest = copy.deepcopy(MANIFEST)
    manifest["routes"] = [{
        "id": "preferences",
        "schemaVersion": 1,
        "method": "GET",
        "path": "/preferences",
        "audience": "user",
        "capabilityId": "catalog.models.read",
        "capabilityVersion": 1,
    }]
    return manifest


def proxy_response(proxy_request: dict, payload: dict) -> dict:
    return {
        key: proxy_request[key]
        for key in (
            "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
            "deploymentGeneration", "requestId", "capabilityId", "capabilityVersion",
            "proxyRequestId",
        )
    } | {
        "result": {"kind": "response", "value": {"statusCode": 200, "payload": payload}}
    }


class PluginSdkTests(unittest.TestCase):
    def plugin(self, handler=None):
        return Plugin(MANIFEST, handler or (lambda *args: {"models": [], "nextCursor": None}), clock_ms=lambda: 1000)

    def test_all_protocol_responses_preserve_the_complete_host_correlation(self):
        for operation, expected in [
            ({"operation": "manifest"}, "manifest"),
            ({"operation": "health"}, "health"),
            ({"operation": "invoke_capability", "connectionId": "catalog", "publicConfig": {}, "capabilityId": "catalog.models.read", "capabilityVersion": 1, "input": {"query": None, "cursor": None, "limit": 20}}, "capability"),
        ]:
            envelope = request(operation)
            response = self.plugin().respond(envelope)
            self.assertEqual(response["result"]["kind"], expected)
            for field in envelope.keys() - {"operation", "deadlineUnixMs"}:
                self.assertEqual(response[field], envelope[field])

    def test_task_operation_returns_bounded_task_projection(self):
        manifest = copy.deepcopy(MANIFEST)
        manifest["capabilities"].append({
            "id": "task.video.adapter", "version": 1, "access": "write",
            "sideEffect": "external_write",
            "writeSemantics": {"nativeIdempotencyKey": True, "resultLookup": "by_request_id", "reconciliation": True},
        })
        task = {
            "kind": "submitted", "value": {
                "task_id": "provider-1", "state": "queued", "progress_basis_points": 0,
            },
        }
        plugin = Plugin(
            manifest,
            lambda *args: {"models": [], "nextCursor": None},
            task_handler=lambda *args: task,
            clock_ms=lambda: 1000,
        )
        response = plugin.respond({
            **request({
                "operation": "invoke_task", "connectionId": "task", "publicConfig": {},
                "capabilityId": "task.video.adapter", "capabilityVersion": 1,
                "task": {"kind": "status", "task_id": "provider-1"},
            }),
            "pluginId": manifest["pluginId"], "pluginVersion": manifest["pluginVersion"],
        })
        self.assertEqual(response["result"]["kind"], "task")
        self.assertEqual(response["result"]["value"], task)

    def test_provider_session_result_is_supported_by_the_python_sdk(self):
        manifest = copy.deepcopy(MANIFEST)
        manifest["capabilities"] = [{
            "id": "provider.lifecycle", "version": 1, "access": "read", "sideEffect": "external_read",
        }]
        session = {
            "kind": "session",
            "session_id": "thread-1",
            "state": {"phase": "polling"},
            "action": {
                "kind": "request", "method": "GET", "path": "/v1/status/thread-1",
                "headers": {}, "body": {}, "stream": False,
            },
        }
        plugin = Plugin(
            manifest,
            lambda *args: session,
            provider_handler=lambda *args: session,
            clock_ms=lambda: 1000,
        )
        response = plugin.respond({
            **request({
                "operation": "invoke_provider", "connectionId": "provider", "publicConfig": {},
                "capabilityId": "provider.lifecycle", "capabilityVersion": 1,
                "provider": {"kind": "session_start", "model": "capy-free-model", "request": {}},
            }, manifest),
        })
        self.assertEqual(response["result"]["kind"], "provider")
        self.assertEqual(response["result"]["value"], session)

    def test_provider_cleanup_request_can_use_a_granted_endpoint_auth_header(self):
        cleanup = {
            "kind": "request", "method": "POST", "endpoint_id": "capy-api",
            "path": "/app/orgs/org/jams/delete",
            "headers": {"authorization": "Bearer jwt", "cookie": "__client=session"},
            "body": {"jamIds": ["child"]},
        }
        Plugin._validate_provider_result(cleanup)
        unscoped = copy.deepcopy(cleanup)
        unscoped.pop("endpoint_id")
        with self.assertRaises(PluginError):
            Plugin._validate_provider_result(unscoped)
        for invalid in (
            {**cleanup, "method": "TRACE"},
            {**cleanup, "path": "/app/../private"},
            {**cleanup, "path": "//other.example/app"},
        ):
            with self.assertRaises(PluginError):
                Plugin._validate_provider_result(invalid)

    def test_provider_websocket_session_actions_are_bounded(self):
        session = {
            "kind": "session",
            "session_id": "thread-1",
            "state": {},
            "action": {
                "kind": "web_socket_send_binary",
                "data_base64": "aGVsbG8=",
            },
        }
        Plugin._validate_provider_result(session)
        for data_base64 in ("%%%",):
            invalid = copy.deepcopy(session)
            invalid["action"]["data_base64"] = data_base64
            with self.assertRaises(PluginError):
                Plugin._validate_provider_result(invalid)

        oversized = copy.deepcopy(session)
        oversized["action"] = {
            "kind": "web_socket_send_text",
            "text": "x" * (128 * 1024 + 1),
        }
        with self.assertRaises(PluginError):
            Plugin._validate_provider_result(oversized)

        batch = copy.deepcopy(session)
        batch["action"] = {
            "kind": "web_socket_receive_batch", "timeout_ms": 1000, "max_messages": 16,
        }
        Plugin._validate_provider_result(batch)
        batch["action"]["max_messages"] = 17
        with self.assertRaises(PluginError):
            Plugin._validate_provider_result(batch)

        auth = copy.deepcopy(session)
        auth["action"] = {
            "kind": "web_socket_authenticate", "request": {"challenge": {"nonce": "n"}},
        }
        Plugin._validate_provider_result(auth)

    def test_credential_websocket_auth_result_is_bounded(self):
        Plugin._validate_credential_result({
            "kind": "web_socket_auth",
            "frames": [{"kind": "text", "text": '{"type":"auth","token":"secret"}'}],
        })
        with self.assertRaises(PluginError):
            Plugin._validate_credential_result({"kind": "web_socket_auth", "frames": []})

    def test_malformed_envelopes_and_non_json_numbers_close_the_stream(self):
        envelope = request({"operation": "health"})
        envelope["deploymentGeneration"] = True
        with self.assertRaises(ProtocolError):
            self.plugin().respond(envelope)
        malformed = request({"operation": "manifest"})
        malformed["extra"] = "rejected"
        self.assertEqual(self.plugin().serve(io.BytesIO(json.dumps(malformed).encode() + b"\n"), io.BytesIO()), 1)
        self.assertEqual(self.plugin().serve(io.BytesIO(b'{"invalid":NaN}\n'), io.BytesIO()), 1)

    def test_legacy_operation_names_are_read_but_duplicate_aliases_are_rejected(self):
        legacy = request({"operation": "invoke_capability", "connection_id": "catalog", "public_config": {}, "capability_id": "catalog.models.read", "capability_version": 1, "input": {"limit": 20}})
        self.assertEqual(self.plugin().respond(legacy)["result"]["kind"], "capability")
        legacy["operation"]["connectionId"] = "other-catalog"
        with self.assertRaises(ProtocolError):
            self.plugin().respond(legacy)

    def test_expired_requests_do_not_call_handlers_and_exceptions_do_not_expose_values(self):
        calls = []

        def broken(*args):
            calls.append(1)
            raise RuntimeError("private-handler-value")

        envelope = request({"operation": "invoke_capability", "connectionId": "catalog", "publicConfig": {}, "capabilityId": "catalog.models.read", "capabilityVersion": 1, "input": {"limit": 20}})
        expired = copy.deepcopy(envelope)
        expired["deadlineUnixMs"] = 999
        self.assertEqual(self.plugin(broken).respond(expired)["result"]["value"]["code"], "temporarily_unavailable")
        self.assertEqual(calls, [])
        result = self.plugin(broken).respond(envelope)
        self.assertEqual(result["result"]["value"]["code"], "upstream_failure")
        self.assertNotIn("private-handler-value", json.dumps(result))

    def test_handler_failures_preserve_operation_outcome_without_exposing_details(self):
        def uncertain(*args):
            raise RuntimeError("private provider response")

        def rejected(*args):
            raise PluginError("temporarily_unavailable", "not_applied")

        operation = {
            "operation": "invoke_capability",
            "connectionId": "catalog",
            "publicConfig": {},
            "capabilityId": "catalog.models.read",
            "capabilityVersion": 1,
            "input": {"query": None, "cursor": None, "limit": 20},
        }
        unknown = self.plugin(uncertain).respond(request(operation))
        explicit = self.plugin(rejected).respond(request(operation))

        self.assertEqual(unknown["result"]["value"]["outcome"], "unknown")
        self.assertEqual(explicit["result"]["value"]["outcome"], "not_applied")
        self.assertNotIn("private provider response", json.dumps(unknown))

    def test_unsupported_capability_is_a_business_error_and_stream_remains_framed(self):
        envelope = request({"operation": "invoke_capability", "connectionId": "catalog", "publicConfig": {}, "capabilityId": "catalog.models.read", "capabilityVersion": 2, "input": {"limit": 20}})
        source = io.BytesIO((json.dumps(envelope) + "\n" + json.dumps(request({"operation": "health"})) + "\n").encode())
        target = io.BytesIO()
        self.assertEqual(self.plugin().serve(source, target), 0)
        responses = [json.loads(line) for line in target.getvalue().splitlines()]
        self.assertEqual(responses[0]["result"]["value"]["code"], "unsupported")
        self.assertEqual(responses[1]["result"]["kind"], "health")

    def test_small_capability_limit_does_not_limit_handshake_or_wire_metadata(self):
        manifest = copy.deepcopy(MANIFEST)
        manifest["limits"]["maxResponseBytes"] = 64
        output = {"value": "x" * 50}
        plugin = Plugin(manifest, lambda *args: output, clock_ms=lambda: 1000)
        operations = [
            {"operation": "manifest"},
            {"operation": "health"},
            {"operation": "invoke_capability", "connectionId": "catalog", "publicConfig": {}, "capabilityId": "catalog.models.read", "capabilityVersion": 1, "input": {"limit": 20}},
        ]
        source = io.BytesIO("".join(json.dumps(request(operation)) + "\n" for operation in operations).encode())
        target = io.BytesIO()
        self.assertEqual(plugin.serve(source, target), 0)
        responses = [json.loads(line) for line in target.getvalue().splitlines()]
        self.assertEqual([response["result"]["kind"] for response in responses], ["manifest", "health", "capability"])
        self.assertEqual(responses[0]["result"]["value"], manifest)
        self.assertEqual(responses[2]["result"]["value"]["output"], output)
        self.assertTrue(all(len(line) > 64 for line in target.getvalue().splitlines()))

    def test_capability_values_remain_bounded_independently_of_wire_frames(self):
        manifest = copy.deepcopy(MANIFEST)
        manifest["limits"]["maxResponseBytes"] = 64
        plugin = Plugin(manifest, lambda *args: {"value": "x" * 100}, clock_ms=lambda: 1000)
        envelope = request({"operation": "invoke_capability", "connectionId": "catalog", "publicConfig": {}, "capabilityId": "catalog.models.read", "capabilityVersion": 1, "input": {"limit": 20}})
        self.assertEqual(plugin.respond(envelope)["result"]["value"]["code"], "invalid_response")
        envelope["operation"]["input"] = {"value": "x" * 100}
        with self.assertRaises(ProtocolError):
            plugin.respond(envelope)
        self.assertEqual(plugin.serve(io.BytesIO(b" " * (1_048_576 + 1) + b"\n"), io.BytesIO()), 1)

    def test_external_read_handler_uses_correlated_host_proxy_frames(self):
        manifest = external_profile_manifest()

        def inspect_profile(capability_id, public_config, value, context):
            self.assertEqual(capability_id, "external.account.profile.read")
            return {"profile": context.proxy_read("account.profile")}

        plugin = Plugin(manifest, inspect_profile, clock_ms=lambda: 1000)
        envelope = request(
            {
                "operation": "invoke_capability",
                "connectionId": "host-owned-scope",
                "publicConfig": {},
                "capabilityId": "external.account.profile.read",
                "capabilityVersion": 1,
                "input": {},
            },
            manifest,
        )
        continuation = proxy_response(
            {
                "protocolVersion": 1,
                "pluginId": envelope["pluginId"],
                "pluginVersion": envelope["pluginVersion"],
                "artifactSha256": envelope["artifactSha256"],
                "deploymentGeneration": envelope["deploymentGeneration"],
                "requestId": envelope["requestId"],
                "capabilityId": "external.account.profile.read",
                "capabilityVersion": 1,
                "proxyRequestId": "proxy-1",
            },
            {"displayName": "Ada"},
        )
        source = io.BytesIO((json.dumps(envelope) + "\n" + json.dumps(continuation) + "\n").encode())
        target = io.BytesIO()
        self.assertEqual(plugin.serve(source, target), 0)
        frames = [json.loads(line) for line in target.getvalue().splitlines()]
        self.assertEqual(len(frames), 2)
        self.assertEqual(frames[0]["proxyRequestId"], "proxy-1")
        self.assertEqual(frames[0]["request"], {
            "endpointId": "account.profile",
            "method": "GET",
            "query": {},
        })
        self.assertNotIn("connectionId", frames[0])
        self.assertNotIn("credential", json.dumps(frames[0]).lower())
        self.assertEqual(frames[1]["result"]["value"]["output"], {"profile": {"displayName": "Ada"}})

    def test_external_read_capability_without_host_context_is_rejected(self):
        manifest = external_profile_manifest()
        plugin = Plugin(
            manifest,
            lambda capability_id, config, value: {"profile": {}},
            clock_ms=lambda: 1000,
        )
        envelope = request(
            {
                "operation": "invoke_capability",
                "connectionId": "host-owned-scope",
                "publicConfig": {},
                "capabilityId": "external.account.profile.read",
                "capabilityVersion": 1,
                "input": {},
            },
            manifest,
        )
        self.assertEqual(plugin.respond(envelope)["result"]["value"]["code"], "unsupported")

    def test_live_review_uses_its_separate_external_balance_grant(self):
        manifest = live_balance_review_manifest()

        observed = []

        def review(capability_id, config, value, context):
            observed.append(context.proxy_read("account.balance"))
            return {"summary": "Reviewed snapshots", "checks": []}

        plugin = Plugin(
            manifest,
            review,
            clock_ms=lambda: 1000,
        )
        envelope = request(
            {
                "operation": "invoke_capability",
                "connectionId": "host-owned-scope",
                "publicConfig": {},
                "capabilityId": "account.balance.snapshot.live-review.read",
                "capabilityVersion": 1,
                "input": {"snapshots": [{"amount": "11.00"}]},
            },
            manifest,
        )
        callback = proxy_response(
            {
                "protocolVersion": 1,
                "pluginId": envelope["pluginId"],
                "pluginVersion": envelope["pluginVersion"],
                "artifactSha256": envelope["artifactSha256"],
                "deploymentGeneration": envelope["deploymentGeneration"],
                "requestId": envelope["requestId"],
                "capabilityId": "external.account.balance.read",
                "capabilityVersion": 1,
                "proxyRequestId": "proxy-1",
            },
            {"amount": "12.50", "unit": "USD", "status": "available", "observedAt": 1700000000},
        )
        source = io.BytesIO((json.dumps(envelope) + "\n" + json.dumps(callback) + "\n").encode())
        target = io.BytesIO()
        self.assertEqual(plugin.serve(source, target), 0)
        frames = [json.loads(line) for line in target.getvalue().splitlines()]
        self.assertEqual(frames[0]["capabilityId"], "external.account.balance.read")
        self.assertEqual(frames[0]["request"]["endpointId"], "account.balance")
        self.assertNotIn("connectionId", frames[0])
        self.assertEqual(frames[1]["result"]["value"]["output"], {"summary": "Reviewed snapshots", "checks": []})
        self.assertEqual(observed[0],
            {"amount": "12.50", "unit": "USD", "status": "available", "observedAt": 1700000000},
        )

    def test_user_route_handler_can_use_host_storage_context(self):
        manifest = plugin_route_manifest()
        observed = []

        def read_preferences(route_id, capability_id, config, value, context):
            observed.append(context.storage({"operation": "get", "key": {"key": "theme"}}))
            return {"theme": observed[-1]}

        plugin = Plugin(
            manifest,
            lambda *args: {},
            route_handler=read_preferences,
            clock_ms=lambda: 1000,
        )
        envelope = request({
            "operation": "plugin_route",
            "connectionId": "host-owned-scope",
            "publicConfig": {},
            "capabilityId": "catalog.models.read",
            "capabilityVersion": 1,
            "routeId": "preferences",
            "input": {},
        }, manifest)
        storage_request = {
            "protocolVersion": 1,
            "pluginId": envelope["pluginId"],
            "pluginVersion": envelope["pluginVersion"],
            "artifactSha256": envelope["artifactSha256"],
            "deploymentGeneration": envelope["deploymentGeneration"],
            "requestId": envelope["requestId"],
            "storageRequestId": "storage-1",
        }
        continuation = storage_request | {
            "result": {"kind": "value", "value": {"value": {"theme": "dark"}}},
        }
        source = io.BytesIO((json.dumps(envelope) + "\n" + json.dumps(continuation) + "\n").encode())
        target = io.BytesIO()

        self.assertEqual(plugin.serve(source, target), 0)
        frames = [json.loads(line) for line in target.getvalue().splitlines()]
        self.assertEqual(frames[0]["operation"], {"operation": "get", "key": {"key": "theme"}})
        self.assertEqual(observed, [{"value": {"theme": "dark"}}])
        self.assertEqual(frames[1]["result"]["value"]["output"], {"theme": {"value": {"theme": "dark"}}})

    def test_external_proxy_rejects_mismatched_continuation_and_stops_stream(self):
        manifest = external_profile_manifest()
        plugin = Plugin(
            manifest,
            lambda capability_id, config, value, context: context.proxy_read("account.profile"),
            clock_ms=lambda: 1000,
        )
        envelope = request(
            {
                "operation": "invoke_capability",
                "connectionId": "host-owned-scope",
                "publicConfig": {},
                "capabilityId": "external.account.profile.read",
                "capabilityVersion": 1,
                "input": {},
            },
            manifest,
        )
        bad = proxy_response(
            {
                "protocolVersion": 1,
                "pluginId": envelope["pluginId"],
                "pluginVersion": envelope["pluginVersion"],
                "artifactSha256": envelope["artifactSha256"],
                "deploymentGeneration": envelope["deploymentGeneration"],
                "requestId": "another-invocation",
                "capabilityId": "external.account.profile.read",
                "capabilityVersion": 1,
                "proxyRequestId": "proxy-1",
            },
            {"displayName": "Ada"},
        )
        target = io.BytesIO()
        source = io.BytesIO((json.dumps(envelope) + "\n" + json.dumps(bad) + "\n").encode())
        self.assertEqual(plugin.serve(source, target), 1)

    def test_external_proxy_caps_callbacks_per_invocation(self):
        manifest = external_profile_manifest()

        def read_many(capability_id, public_config, value, context):
            for _ in range(5):
                context.proxy_read("account.profile")
            return {"ok": True}

        plugin = Plugin(manifest, read_many, clock_ms=lambda: 1000)
        envelope = request(
            {
                "operation": "invoke_capability",
                "connectionId": "host-owned-scope",
                "publicConfig": {},
                "capabilityId": "external.account.profile.read",
                "capabilityVersion": 1,
                "input": {},
            },
            manifest,
        )
        base_proxy_request = {
            "protocolVersion": 1,
            "pluginId": envelope["pluginId"],
            "pluginVersion": envelope["pluginVersion"],
            "artifactSha256": envelope["artifactSha256"],
            "deploymentGeneration": envelope["deploymentGeneration"],
            "requestId": envelope["requestId"],
            "capabilityId": "external.account.profile.read",
            "capabilityVersion": 1,
        }
        callbacks = [
            proxy_response(base_proxy_request | {"proxyRequestId": f"proxy-{index}"}, {"ok": True})
            for index in range(1, 5)
        ]
        source = io.BytesIO(("\n".join(json.dumps(frame) for frame in [envelope, *callbacks]) + "\n").encode())
        target = io.BytesIO()
        self.assertEqual(plugin.serve(source, target), 0)
        frames = [json.loads(line) for line in target.getvalue().splitlines()]
        self.assertEqual([frame["proxyRequestId"] for frame in frames[:-1]], [f"proxy-{index}" for index in range(1, 5)])
        self.assertEqual(frames[-1]["result"]["value"]["code"], "rate_limited")

    def test_catalogue_example_searches_and_paginates_public_data(self):
        spec = importlib.util.spec_from_file_location("catalogue_example", ROOT / "examples" / "catalog" / "plugin.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        first = module.read_catalog("catalog.models.read", {}, {"limit": 1})
        second = module.read_catalog("catalog.models.read", {}, {"cursor": first["nextCursor"], "limit": 1})
        self.assertNotEqual(first["models"][0]["id"], second["models"][0]["id"])
        self.assertIsNone(second["nextCursor"])
        searched = module.read_catalog("catalog.models.search", {}, {"query": "vision", "limit": 20})
        self.assertEqual([model["id"] for model in searched["models"]], ["example-vision"])


if __name__ == "__main__":
    unittest.main()
