"""Exercise metadata fallback in the actual synchronization workflow scripts."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[3]
SHA = "a" * 40
BODY = "First paragraph.\n\n- Second paragraph.\n"


def workflow_script(name, step):
    workflow = (ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8")
    section = workflow.split(f"      - name: {step}\n", 1)[1]
    lines = []
    for line in section.split("        run: |\n", 1)[1].splitlines():
        if line.strip() and not line.startswith("          "):
            break
        lines.append(line)
    return textwrap.dedent("\n".join(lines))


class MetadataFallbackTests(unittest.TestCase):
    def run_metadata(self, direction, response, http_exit=0):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / "bin"
            binaries.mkdir()
            response_file = root / "response.json"
            response_file.write_text(response, encoding="utf-8")
            mocks = {
                "curl": 'cat "$MOCK_RESPONSE_FILE"; exit "$MOCK_HTTP_EXIT"\n',
                "git": "printf '%s\\n' 'Merge pull request (#322)'\n",
            }
            for name, body in mocks.items():
                path = binaries / name
                path.write_text("#!/bin/sh\n" + body, encoding="utf-8")
                path.chmod(0o700)
            environment = {
                **os.environ,
                "PATH": str(binaries) + os.pathsep + os.environ["PATH"],
                "RUNNER_TEMP": str(root),
                "MOCK_RESPONSE_FILE": str(response_file),
                "MOCK_HTTP_EXIT": str(http_exit),
                "GITEA_COMMIT": SHA,
                "GITEA_API_BASE": "https://gitea.invalid/api/v1",
                "GITEA_REPOSITORY": "acmecloud/AnyFlows",
                "GITEA_API_TOKEN": "test-token",
                "GITHUB_TOKEN": "test-token",
                "PUBLIC_PR_NUMBER": "322",
                "PUBLIC_PR_URL": "https://github.invalid/pull/322",
                "PUBLIC_PATCH_API_URL": "https://github.invalid/api/pulls/322",
            }
            if direction == "gitea":
                script = workflow_script("export-public-core.yml", "Read source PR context")
            else:
                script = workflow_script("propose-gitea-sync.yml", "Prepare public patch")
                # Patch download remains fatal; isolate the optional metadata lookup.
                script = script[script.index('metadata="$RUNNER_TEMP/source-pr-metadata.json"'):]
                script = "set -euo pipefail\n" + script.split("echo 'has_changes=true'", 1)[0]
            result = subprocess.run(
                ["bash", "-c", script], cwd=root, env=environment,
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            metadata = json.loads((root / "source-pr-metadata.json").read_text(encoding="utf-8"))
            self.assertFalse((root / "source-pr-metadata.json.tmp").exists())
            return metadata, result.stdout + result.stderr

    def valid_response(self):
        return {
            "number": 322, "title": "Source title", "body": BODY,
            "html_url": "https://gitea.invalid/pulls/322",
            "merged": True, "merge_commit_sha": SHA, "base": {"ref": "develop"},
        }

    def test_available_metadata_preserves_source_text(self):
        for direction in ("gitea", "github"):
            with self.subTest(direction=direction):
                metadata, output = self.run_metadata(direction, json.dumps(self.valid_response()))
                self.assertEqual(metadata["title"], "Source title")
                self.assertEqual(metadata["body"], BODY)
                self.assertNotIn("::warning::", output)

    def test_failed_or_invalid_lookup_keeps_defaults(self):
        for direction in ("gitea", "github"):
            for response, status in (("", 22), ("", 28), ("not JSON", 0), ("{}", 0), ("null", 0)):
                with self.subTest(direction=direction, response=response, status=status):
                    metadata, output = self.run_metadata(direction, response, status)
                    expected_title = ("同步 Gitea 公共核心" if direction == "gitea"
                                      else "同步 GitHub 公共核心变更 #322")
                    self.assertEqual(metadata["title"], expected_title)
                    self.assertEqual(metadata["body"], "")
                    self.assertIn("::warning::", output)
                    if direction == "gitea":
                        self.assertIsNone(metadata["number"])
                        self.assertEqual(metadata["mergeCommitSha"], "")

    def test_unrelated_private_pr_cannot_supply_comment_mapping(self):
        for field, value in (("number", 99), ("merge_commit_sha", "b" * 40),
                             ("merged", False), ("base", {"ref": "master"})):
            with self.subTest(field=field):
                response = {**self.valid_response(), field: value}
                metadata, _ = self.run_metadata("gitea", json.dumps(response))
                self.assertIsNone(metadata["number"])

    def test_invalid_title_or_body_does_not_replace_defaults(self):
        for direction in ("gitea", "github"):
            for field, value in (("title", ""), ("title", None), ("body", [])):
                with self.subTest(direction=direction, field=field):
                    response = {**self.valid_response(), field: value}
                    metadata, output = self.run_metadata(direction, json.dumps(response))
                    self.assertNotEqual(metadata["title"], response["title"])
                    self.assertEqual(metadata["body"], "")
                    self.assertIn("::warning::", output)

    def test_reused_export_pr_clears_stale_mapping_when_lookup_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / "bin"
            binaries.mkdir()
            payload_file = root / "payload.json"
            existing_file = root / "existing.json"
            existing_file.write_text(json.dumps([{
                "number": 42,
                "body": "<!-- anyflows-pr-source:gitea:acmecloud/AnyFlows:321:"
                        + "b" * 40 + " -->",
            }]), encoding="utf-8")
            (root / "source-pr-metadata.json").write_text(json.dumps({
                "title": "同步 Gitea 公共核心", "body": "", "url": "",
                "number": None, "mergeCommitSha": "",
            }), encoding="utf-8")
            curl = binaries / "curl"
            curl.write_text(textwrap.dedent("""\
                #!/bin/sh
                case "$*" in
                  *"--request PATCH"*)
                    while [ "$#" -gt 0 ]; do
                      if [ "$1" = --data ]; then
                        printf '%s' "$2" > "$MOCK_PAYLOAD_FILE"
                        exit 0
                      fi
                      shift
                    done
                    exit 1
                    ;;
                  *) cat "$MOCK_EXISTING_FILE" ;;
                esac
                """), encoding="utf-8")
            curl.chmod(0o700)
            environment = {
                **os.environ,
                "PATH": str(binaries) + os.pathsep + os.environ["PATH"],
                "RUNNER_TEMP": str(root),
                "MOCK_PAYLOAD_FILE": str(payload_file),
                "MOCK_EXISTING_FILE": str(existing_file),
                "GITEA_COMMIT": SHA,
                "GITEA_REPOSITORY": "acmecloud/AnyFlows",
                "GITEA_SOURCE_REPOSITORY_URL": "https://gitea.invalid/acmecloud/AnyFlows",
                "GITHUB_PUBLIC_REPOSITORY": "ChuqiCloud/AnyFlows",
                "GITHUB_PUBLIC_BRANCH": "main",
                "PUBLIC_REPO_SYNC_TOKEN": "test-token",
            }
            script = workflow_script("export-public-core.yml", "Create or reuse GitHub pull request")
            script = script.replace("${{ steps.branch.outputs.export_branch }}",
                                    "sync/gitea-public-core")
            result = subprocess.run(["bash", "-c", script], cwd=root, env=environment,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            payload = json.loads(payload_file.read_text(encoding="utf-8"))
            self.assertEqual(payload["title"], "同步 Gitea 公共核心")
            self.assertIn(SHA, payload["body"])
            self.assertNotIn("anyflows-pr-source", payload["body"])


if __name__ == "__main__":
    unittest.main()
