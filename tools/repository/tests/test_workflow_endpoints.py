"""Keep checkout endpoints and privileged synchronization gates portable."""

import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[3]
CHECKOUT_BASE = "${{ github.server_url != 'https://github.com' && vars.SOURCE_BASE_URL || github.server_url }}"


def workflow(name):
    return (ROOT / ".github/workflows" / name).read_text(encoding="utf-8")


def script(name, step):
    section = workflow(name).split(f"      - name: {step}\n", 1)[1]
    lines = []
    for line in section.split("        run: |\n", 1)[1].splitlines():
        if line.strip() and not line.startswith("          "):
            break
        lines.append(line)
    return textwrap.dedent("\n".join(lines))


class WorkflowEndpointTests(unittest.TestCase):
    def test_public_checkout_does_not_use_the_return_sync_endpoint(self):
        for name in ("build-preview.yml", "conversation-compatibility.yml", "console-extensions.yml",
                     "verify-preview-opencode.yml", "verify-preview-pi.yml"):
            with self.subTest(workflow=name):
                source = workflow(name)
                self.assertIn(CHECKOUT_BASE + "/${{ github.repository }}.git", source)
                self.assertNotIn("secrets.GITEA_BASE_URL", source)
        for name in ("verify-preview-opencode.yml", "verify-preview-pi.yml"):
            self.assertIn(
                "SOURCE_API: ${{ github.server_url != 'https://github.com' && vars.SOURCE_BASE_URL && "
                "format('{0}/api/v1', vars.SOURCE_BASE_URL) || github.api_url }}",
                workflow(name),
            )

    def test_git_url_rewrite_normalizes_slashes_and_only_runs_off_github(self):
        for name in ("ci.yml", "frontend-generated-assets.yml", "proxy-pools.yml"):
            with self.subTest(workflow=name):
                source = workflow(name)
                self.assertIn(
                    "if: ${{ github.server_url != 'https://github.com' && vars.SOURCE_BASE_URL != '' }}",
                    source,
                )
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    binary = root / "git"
                    binary.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$MOCK_GIT_ARGS"\n', encoding="utf-8")
                    binary.chmod(0o700)
                    result = subprocess.run(
                        ["bash", "-c", script(name, "Configure checkout endpoint")],
                        cwd=root,
                        env={**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
                             "CHECKOUT_SERVER_URL": "http://source.internal:3000/",
                             "CHECKOUT_EXTERNAL_URL": "https://example.test/",
                             "MOCK_GIT_ARGS": str(root / "arguments")},
                        capture_output=True, text=True,
                    )
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual((root / "arguments").read_text().splitlines(), [
                        "config", "--global", "url.https://example.test/.insteadOf", "http://source.internal:3000/",
                    ])

    def test_source_sync_requires_configured_repository_on_non_github_server(self):
        for name in ("export-public-core.yml", "mirror-gitea-pr-discussion.yml"):
            source = workflow(name)
            for guard in ("github.server_url != 'https://github.com'", "vars.SOURCE_REPOSITORY != ''",
                          "github.repository == vars.SOURCE_REPOSITORY"):
                self.assertIn(guard, source)
            self.assertIn("GITEA_REPOSITORY: ${{ github.repository }}", source)

    def test_return_sync_retains_trusted_event_and_revision_guards(self):
        source = workflow("propose-gitea-sync.yml")
        self.assertEqual(source.count("github.server_url == 'https://github.com'"), 2)
        self.assertEqual(source.count("github.repository == 'ChuqiCloud/AnyFlows'"), 2)
        self.assertEqual(source.count("GITEA_BASE_URL: ${{ secrets.GITEA_BASE_URL }}"), 2)
        self.assertEqual(source.count("GITEA_REPOSITORY: ${{ secrets.GITEA_REPOSITORY }}"), 2)
        for guard in ("github.event.pull_request.merged == true", "github.event.pull_request.base.ref == 'main'",
                      "!startsWith(github.event.pull_request.head.ref, 'sync/gitea-public-core')",
                      "github.event.comment.user.type != 'Bot'", "!endsWith(github.event.comment.user.login, '[bot]')",
                      "ref: ${{ github.sha }}", "persist-credentials: false"):
            self.assertIn(guard, source)
        self.assertNotIn("ref: ${{ github.event.pull_request.head.sha }}", source)

    def test_missing_return_configuration_fails_without_printing_values(self):
        complete = {"GITEA_BASE_URL": "https://example.test", "GITEA_REPOSITORY": "source/project",
                    "GITEA_USERNAME": "sync-user", "GITEA_TOKEN": "test-token"}
        for missing in (None, "GITEA_BASE_URL", "GITEA_REPOSITORY", "GITEA_USERNAME", "GITEA_TOKEN"):
            with self.subTest(missing=missing):
                values = dict(complete)
                if missing:
                    values[missing] = ""
                result = subprocess.run(
                    ["bash", "-c", script("propose-gitea-sync.yml", "Validate sync credentials")],
                    env={**os.environ, **values}, capture_output=True, text=True,
                )
                self.assertEqual(result.returncode == 0, missing is None)
                output = result.stdout + result.stderr
                for value in complete.values():
                    self.assertNotIn(value, output)


if __name__ == "__main__":
    unittest.main()
