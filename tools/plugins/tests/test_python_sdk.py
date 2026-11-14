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
from anyflows_plugin import Plugin, ProtocolError  # noqa: E402


MANIFEST = json.loads((ROOT / "examples" / "catalog" / "manifest.json").read_text(encoding="utf-8"))


def request(operation: dict) -> dict:
    return {
        "protocolVersion": 1,
        "pluginId": MANIFEST["pluginId"],
        "pluginVersion": MANIFEST["pluginVersion"],
        "artifactSha256": "a" * 64,
        "deploymentGeneration": 7,
        "requestId": "host-request-1",
        "deadlineUnixMs": 2000,
        "operation": operation,
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
