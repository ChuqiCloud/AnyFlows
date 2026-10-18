"""Content-level regression checks for independent public/source histories."""

from importlib.util import module_from_spec, spec_from_file_location
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SPEC = spec_from_file_location("public_export", Path(__file__).parents[1] / "export-public-core.py")
assert SPEC and SPEC.loader
EXPORT = module_from_spec(SPEC)
SPEC.loader.exec_module(EXPORT)
METADATA = "docs/public-core-source.json"


def git(repository, *arguments):
    return subprocess.check_output(
        ["git", "-C", str(repository), *arguments], text=True, encoding="utf-8"
    ).strip()


class PublicSyncTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.source, self.public = root / "source", root / "public"
        for repository in (self.source, self.public):
            repository.mkdir()
            git(repository, "init", "--quiet")
            git(repository, "config", "user.name", "ci")
            git(repository, "config", "user.email", "ci@example.invalid")
            self.write(repository, "Cargo.toml", 'rustls = "0.23.45"\n')
            self.write(repository, "payment.rs", "// core orders\n")
            if repository == self.public:
                self.write(repository, METADATA, '{"sourceCommit":"initial"}\n')
            self.commit(repository)

    def write(self, repository, name, text):
        path = repository / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit(self, repository):
        git(repository, "add", "-A")
        git(repository, "commit", "--quiet", "-m", "fixture")

    def verify(self):
        EXPORT.verify_public_sync(
            self.source, self.public, git(self.source, "rev-parse", "HEAD"), METADATA, {"LICENSE"}
        )

    def test_missing_security_patch_and_payment_extension_are_rejected(self):
        self.write(self.public, "Cargo.toml", 'rustls = "0.23.46"\n')
        self.write(self.public, "payment.rs", "// core and extension orders\n")
        self.commit(self.public)
        # A later, unrelated contribution must not conceal the missing one.
        for repository in (self.source, self.public):
            self.write(repository, "account.rs", "// new account API\n")
            self.commit(repository)
        before = (self.source / "Cargo.toml").read_bytes()
        with self.assertRaisesRegex(EXPORT.ExportError, "have not been reconciled"):
            self.verify()
        self.assertEqual((self.source / "Cargo.toml").read_bytes(), before)
        self.assertEqual(git(self.source, "status", "--porcelain"), "")
        self.assertEqual(git(self.public, "status", "--porcelain"), "")

    def test_reconciled_public_changes_allow_additional_source_changes(self):
        for repository in (self.source, self.public):
            self.write(repository, "Cargo.toml", 'rustls = "0.23.46"\n')
            self.write(repository, "payment.rs", "// core and extension orders\n")
            self.commit(repository)
        self.write(self.source, "new.rs", "// source change\n")
        self.commit(self.source)
        self.verify()

    def test_reconciled_rewrite_allows_source_followup_in_same_file(self):
        rewritten = "# public deployment guide\n\nFollow the release checklist.\n"
        self.write(self.public, "payment.rs", rewritten)
        self.commit(self.public)
        self.write(self.source, "payment.rs", rewritten + "\nEnterprise-only release notes stay in the source repository.\n")
        self.commit(self.source)
        self.verify()

    def test_reconciled_public_delta_allows_source_edit_inside_public_file(self):
        rewritten = "# public deployment guide\n\nFollow the release checklist.\n"
        self.write(self.public, "payment.rs", rewritten)
        self.commit(self.public)
        self.write(
            self.source,
            "payment.rs",
            "# public deployment guide\n\nEnterprise release notes.\n\nFollow the release checklist.\n",
        )
        self.commit(self.source)
        self.verify()

    def test_reconciled_public_delta_ignores_final_newline_difference(self):
        self.write(self.public, "payment.rs", "# public deployment guide\n\nFollow the release checklist.")
        self.commit(self.public)
        self.write(
            self.source,
            "payment.rs",
            "# public deployment guide\n\nFollow the release checklist.\nEnterprise release notes.\n",
        )
        self.commit(self.source)
        self.verify()

    def test_conflicting_public_change_is_rejected(self):
        self.write(self.public, "payment.rs", "// public order routing\n")
        self.commit(self.public)
        self.write(self.source, "payment.rs", "// divergent source routing\n")
        self.commit(self.source)
        with self.assertRaisesRegex(EXPORT.ExportError, "payment.rs"):
            self.verify()

    def test_public_deletion_cannot_be_restored_by_stale_source(self):
        (self.public / "payment.rs").unlink()
        self.commit(self.public)
        with self.assertRaises(EXPORT.ExportError):
            self.verify()
        (self.source / "payment.rs").unlink()
        self.commit(self.source)
        self.verify()

    def test_public_license_is_preserved_independently(self):
        self.write(self.public, "LICENSE", "public license\n")
        self.commit(self.public)
        self.verify()

    def test_verification_does_not_start_background_maintenance(self):
        root = Path(self.directory.name)
        config = root / "gitconfig"
        trace = root / "git-trace.jsonl"
        config.write_text(
            "[maintenance]\n\tauto = true\n[gc]\n\tauto = 1\n\tautoDetach = true\n",
            encoding="utf-8",
        )
        with patch.dict(os.environ, {
            "GIT_CONFIG_GLOBAL": str(config),
            "GIT_TRACE2_EVENT": str(trace),
        }):
            self.verify()
        events = [json.loads(line) for line in trace.read_text(encoding="utf-8").splitlines()]
        children = [event.get("argv", []) for event in events if event["event"] == "child_start"]
        self.assertTrue(children, "Git trace must capture the comparison subprocesses")
        self.assertFalse(
            [args for args in children if "maintenance" in args or "gc" in args],
            "Disposable comparison repositories must not launch automatic maintenance",
        )

    def test_initial_empty_repository_is_allowed(self):
        for path in (self.public / "Cargo.toml", self.public / "payment.rs", self.public / METADATA):
            path.unlink()
        self.commit(self.public)
        self.verify()

    def test_existing_repository_without_provenance_is_rejected(self):
        (self.public / METADATA).unlink()
        self.commit(self.public)
        with self.assertRaisesRegex(EXPORT.ExportError, "no export provenance"):
            self.verify()


if __name__ == "__main__":
    unittest.main()
