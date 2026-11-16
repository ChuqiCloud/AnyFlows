"""Packaging regressions for the self-contained example plugins."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]
PACKAGER = ROOT / "package-example.py"


class ExamplePackageTests(unittest.TestCase):
    def package(self, output: Path, example: str | None = None) -> dict:
        command = [sys.executable, str(PACKAGER), str(output)]
        if example is not None:
            command.extend(["--example", example])
        subprocess.run(command, check=True, capture_output=True, text=True)
        with zipfile.ZipFile(output) as archive:
            return {
                "names": set(archive.namelist()),
                "metadata": json.loads(archive.read("anyflows-plugin.json")),
            }

    def test_merchant_review_example_contains_the_python_sdk(self):
        with tempfile.TemporaryDirectory(prefix="anyflows-example-test-") as temporary:
            output = Path(temporary) / "merchant-review.afplugin"
            package = self.package(output, "merchant-service-review")
            self.assertIn("payload/anyflows_plugin.py", package["names"])
            self.assertIn("payload/plugin.py", package["names"])
            self.assertEqual(
                package["metadata"]["manifest"]["capabilities"][0]["id"],
                "merchant.service.review.read",
            )

    def test_channel_monitor_review_example_contains_the_python_sdk(self):
        with tempfile.TemporaryDirectory(prefix="anyflows-example-test-") as temporary:
            output = Path(temporary) / "channel-monitor-review.afplugin"
            package = self.package(output, "channel-monitor-review")
            self.assertIn("payload/anyflows_plugin.py", package["names"])
            self.assertIn("payload/plugin.py", package["names"])
            self.assertEqual(
                package["metadata"]["manifest"]["capabilities"][0]["id"],
                "channel.monitor.events.review.read",
            )

    def test_durable_channel_health_event_example_contains_the_python_sdk(self):
        with tempfile.TemporaryDirectory(prefix="anyflows-example-test-") as temporary:
            output = Path(temporary) / "channel-health-event.afplugin"
            package = self.package(output, "channel-health-event")
            self.assertIn("payload/anyflows_plugin.py", package["names"])
            self.assertIn("payload/plugin.py", package["names"])
            self.assertEqual(
                package["metadata"]["manifest"]["capabilities"][0]["id"],
                "events.channel-health.changed.consume",
            )
            self.assertEqual(
                package["metadata"]["manifest"]["uiContributions"][0]["slot"],
                "plugin_management",
            )

    def test_channel_model_mapping_review_example_contains_the_python_sdk(self):
        with tempfile.TemporaryDirectory(prefix="anyflows-example-test-") as temporary:
            output = Path(temporary) / "channel-model-mapping-review.afplugin"
            package = self.package(output, "channel-model-mapping-review")
            self.assertIn("payload/anyflows_plugin.py", package["names"])
            self.assertIn("payload/plugin.py", package["names"])
            self.assertEqual(
                package["metadata"]["manifest"]["capabilities"][0]["id"],
                "channel.model_mapping.review.read",
            )

    def test_catalog_example_remains_the_default(self):
        with tempfile.TemporaryDirectory(prefix="anyflows-example-test-") as temporary:
            output = Path(temporary) / "catalog.afplugin"
            package = self.package(output)
            self.assertEqual(
                package["metadata"]["manifest"]["pluginId"],
                "org.anyflows.example-catalog",
            )
            self.assertIn("payload/anyflows_plugin.py", package["names"])

    def test_catalog_query_uses_a_safe_utf8_byte_limit(self):
        with tempfile.TemporaryDirectory(prefix="anyflows-example-test-") as temporary:
            output = Path(temporary) / "catalog.afplugin"
            package = self.package(output)
            query_limits = [
                capability["inputSchema"]["properties"]["query"]["maxLength"]
                for capability in package["metadata"]["manifest"]["capabilities"]
            ]
            self.assertEqual(query_limits, [128, 128])

        catalog = ROOT / "examples" / "catalog"
        spec = importlib.util.spec_from_file_location(
            "anyflows_catalog_example", catalog / "plugin.py"
        )
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        sys.path.insert(0, str(catalog))
        try:
            spec.loader.exec_module(module)
        finally:
            sys.path.pop(0)

        with self.assertRaises(module.PluginError):
            module.read_catalog(
                "catalog.models.search", {}, {"query": "🙂" * 33, "limit": 10}
            )
        self.assertEqual(
            module.read_catalog(
                "catalog.models.search", {}, {"query": "🙂" * 32, "limit": 10}
            )["models"],
            [],
        )


if __name__ == "__main__":
    unittest.main()
