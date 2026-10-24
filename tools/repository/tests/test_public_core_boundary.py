"""Check export exclusions and the public documentation boundary."""

import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "tools/repository/public-core-export.json"
VERIFIER = ROOT / "tools/ci/verify-public-core-boundary.py"
DOC_BOUNDARY = re.compile(
    r"\bgitea\b|\benterprise\b|企业版|企业扩展|私有仓库|"
    r"\b[\w.-]+-distribution\b|repository-boundary\.md|public-core-extraction\.json",
    re.IGNORECASE,
)


class PublicBoundaryTests(unittest.TestCase):
    def check(self, root, manifest):
        policy = root / "policy.json"
        policy.write_text(json.dumps(manifest), encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(VERIFIER), "--root", str(root),
             "--manifest", str(policy), "--mode", "public"],
            check=False, capture_output=True, text=True, encoding="utf-8",
        )

    def test_export_manifest_contains_only_public_configuration(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        self.assertEqual(
            set(manifest), {"schemaVersion", "publicRepository", "excludedPaths", "export"}
        )
        self.assertEqual(manifest["publicRepository"], "https://github.com/ChuqiCloud/AnyFlows.git")
        self.assertTrue(manifest["excludedPaths"])
        self.assertIsNone(DOC_BOUNDARY.search(MANIFEST.read_text(encoding="utf-8")))
        self.assertFalse((ROOT / "docs/public-core-extraction.json").exists())

    def test_public_checkout_passes_without_excluded_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.check(Path(directory), {"excludedPaths": ["restricted/*.rs"]})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_glob_and_directory_exclusions_are_enforced(self):
        for pattern in ("restricted/*.rs", "restricted/"):
            with self.subTest(pattern=pattern), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "restricted").mkdir()
                (root / "restricted/private.rs").write_text("fixture\n", encoding="utf-8")
                result = self.check(root, {"excludedPaths": [pattern]})
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn("restricted/private.rs", result.stderr)

    def test_repository_policy_rejects_excluded_source_and_frontend_files(self):
        paths = [
            "crates/af-domain/src/organization.rs",
            "crates/af-db/src/migration/m20260101_000061_example.rs",
            "crates/af-account/src/organization_example.rs",
            "crates/af-http/src/scim.rs",
            "web/src/features/organization-example/index.ts",
            "web-next/src/features/organization-example/index.ts",
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in paths:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture\n", encoding="utf-8")
            result = self.check(root, json.loads(MANIFEST.read_text(encoding="utf-8")))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        for name in paths:
            self.assertIn(name, result.stderr)

    def test_missing_or_invalid_exclusions_fail_closed(self):
        for manifest in ({}, {"excludedPaths": None}, {"excludedPaths": [1]},
                         {"excludedPaths": [""]}, []):
            with self.subTest(manifest=manifest), tempfile.TemporaryDirectory() as directory:
                result = self.check(Path(directory), manifest)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)

    def test_public_documentation_has_no_internal_references(self):
        paths = sorted([*ROOT.glob("*.md"), *(ROOT / "docs").rglob("*.md")])
        self.assertTrue(paths)
        violations = [
            f"{path.relative_to(ROOT).as_posix()}:{line_number}"
            for path in paths
            for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1)
            if DOC_BOUNDARY.search(line)
        ]
        self.assertEqual(violations, [], "internal references in public documentation")

    def test_runtime_help_describes_only_public_metrics(self):
        for frontend in ("web", "web-next"):
            for language in ("zh", "en"):
                path = ROOT / frontend / "src/i18n/locales" / f"{language}.json"
                with self.subTest(path=path.relative_to(ROOT).as_posix()):
                    hint = json.loads(path.read_text(encoding="utf-8"))["runtimeStatus"]["cacheHint"]
                    self.assertIsNone(DOC_BOUNDARY.search(hint))
                    self.assertNotIn("extension-owned", hint)

    def test_visible_verification_help_describes_account_verification(self):
        forbidden = re.compile(r"企业空间|企业管理|workspace|enterprise management", re.IGNORECASE)
        keys = (
            "description", "adminDescription", "workspaceHistory", "enterpriseGranted",
            "applyWorkspace", "enterWorkspace", "verifyFirst", "verifyFirstHint", "organizations",
        )
        for frontend in ("web", "web-next"):
            for language in ("zh", "en"):
                path = ROOT / frontend / "src/i18n/locales" / f"{language}.json"
                with self.subTest(path=path.relative_to(ROOT).as_posix()):
                    locale = json.loads(path.read_text(encoding="utf-8"))
                    texts = [locale["verificationCenter"][key] for key in keys]
                    texts.append(json.dumps(locale["registration"]["enterprise"], ensure_ascii=False))
                    for text in texts:
                        self.assertIsNone(DOC_BOUNDARY.search(text))
                        self.assertIsNone(forbidden.search(text))


if __name__ == "__main__":
    unittest.main()
