"""Regression checks for one-way GitHub-to-Gitea PR comment mirroring."""

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import unittest


SPEC = spec_from_file_location(
    "pr_comment_sync", Path(__file__).parents[1] / "sync-github-pr-comments.py"
)
assert SPEC and SPEC.loader
SYNC = module_from_spec(SPEC)
SPEC.loader.exec_module(SYNC)

SHA = "a" * 40
PUBLIC_REPOSITORY = "ChuqiCloud/AnyFlows"
PRIVATE_REPOSITORY = "source/project"
SYNC_USERNAME = "sync-bot"


def github_pr():
    return {
        "number": 42,
        "base": {"ref": "main"},
        "head": {"ref": f"sync/github-pr-42-{SHA}", "sha": SHA},
        "merged_at": "2026-10-18T00:00:00Z",
        "html_url": "https://github.com/ChuqiCloud/AnyFlows/pull/42",
    }


class GithubApi:
    def __init__(self, comments):
        self.comments = comments

    def request(self, method, path, body=None):
        if method == "GET" and path.endswith("/pulls/42"):
            return github_pr()
        raise AssertionError(f"Unexpected GitHub request: {method} {path}")

    def pages(self, path, size_parameter, size):
        if path.endswith("/issues/42/comments"):
            return list(self.comments)
        raise AssertionError(f"Unexpected GitHub page request: {path}")


class GiteaApi:
    def __init__(self, comments):
        self.comments = {comment["id"]: dict(comment) for comment in comments}
        self.next_id = max(self.comments, default=600) + 1
        self.requests = []

    def request(self, method, path, body=None):
        self.requests.append((method, path, body))
        if method == "GET" and path == "/user":
            return {"login": SYNC_USERNAME}
        if method == "DELETE" and path.startswith("/repos/source/project/issues/comments/"):
            self.comments.pop(int(path.rsplit("/", 1)[1]), None)
            return None
        if method == "PATCH" and path.startswith("/repos/source/project/issues/comments/"):
            comment = self.comments[int(path.rsplit("/", 1)[1])]
            comment.update(body)
            return comment
        if method == "POST" and path == "/repos/source/project/issues/55/comments":
            comment = {"id": self.next_id, "user": {"login": SYNC_USERNAME}, **body}
            self.next_id += 1
            self.comments[comment["id"]] = comment
            return comment
        raise AssertionError(f"Unexpected Gitea request: {method} {path}")

    def pages(self, path, size_parameter, size):
        if path == "/repos/source/project/pulls?state=all":
            return [{
                "number": 55,
                "head": {"ref": f"sync/github-pr-42-{SHA}"},
                "base": {"ref": "develop"},
            }]
        if path == "/repos/source/project/issues/55/comments":
            return list(self.comments.values())
        raise AssertionError(f"Unexpected Gitea page request: {path}")


class CommentSyncTests(unittest.TestCase):
    def test_source_filter_ignores_bots_and_existing_mirrors(self):
        self.assertTrue(SYNC.source_comment({"user": {"login": "alice"}, "body": "Review"}))
        self.assertFalse(SYNC.source_comment({"user": {"type": "Bot"}, "body": "Build"}))
        self.assertFalse(SYNC.source_comment({"user": {"login": "sync[bot]"}, "body": "Build"}))
        self.assertFalse(SYNC.source_comment({
            "user": {"login": "alice"},
            "body": "<!-- anyflows-pr-sync-comment:github:42:12 -->",
        }))

    def test_only_sync_account_comments_are_managed(self):
        mirrored = {
            "user": {"login": "sync-bot"},
            "body": "copy\n\n<!-- anyflows-pr-sync-comment:github:42:12 -->",
        }
        forged = {
            "user": {"login": "reviewer"},
            "body": "private\n\n<!-- anyflows-pr-sync-comment:github:42:12 -->",
        }
        self.assertEqual(SYNC.source_id(mirrored, 42, SYNC_USERNAME), 12)
        self.assertIsNone(SYNC.source_id(forged, 42, SYNC_USERNAME))
        self.assertIsNone(SYNC.source_id(mirrored, 43, SYNC_USERNAME))

    def test_regular_public_pr_requires_exact_gitea_branch(self):
        pr = github_pr()
        candidate = {
            "number": 55,
            "head": {"ref": f"sync/github-pr-42-{SHA}"},
            "base": {"ref": "develop"},
        }

        class Pulls:
            def pages(self, *args):
                return [candidate]

        self.assertEqual(SYNC.find_target(Pulls(), PRIVATE_REPOSITORY, pr, PUBLIC_REPOSITORY), 55)
        candidate["head"]["ref"] = f"sync/github-pr-42-{SHA[:20]}-retry"
        self.assertIsNone(SYNC.find_target(Pulls(), PRIVATE_REPOSITORY, pr, PUBLIC_REPOSITORY))

    def test_export_pr_requires_matching_private_merge(self):
        source_sha = "b" * 40
        pr = {
            "number": 90,
            "base": {"ref": "main"},
            "head": {"ref": SYNC.EXPORT_BRANCH, "repo": {"full_name": PUBLIC_REPOSITORY}},
            "body": (
                "Export details\n\n<!-- anyflows-pr-source:gitea:"
                f"{PRIVATE_REPOSITORY}:17:{source_sha} -->"
            ),
        }

        class PrivatePull:
            def request(self, method, path):
                self.asserted_path = path
                return {
                    "number": 17,
                    "merged": True,
                    "merge_commit_sha": source_sha,
                    "base": {"ref": "develop"},
                }

        self.assertEqual(SYNC.find_target(PrivatePull(), PRIVATE_REPOSITORY, pr, PUBLIC_REPOSITORY), 17)
        pr["body"] = pr["body"].replace(source_sha, "c" * 40)
        with self.assertRaisesRegex(SYNC.SyncError, "does not match"):
            SYNC.find_target(PrivatePull(), PRIVATE_REPOSITORY, pr, PUBLIC_REPOSITORY)

    def test_v2_export_marker_keeps_merge_and_repository_guards(self):
        source_sha = "b" * 40
        pr = {
            "number": 90,
            "base": {"ref": "main"},
            "head": {"ref": SYNC.EXPORT_BRANCH, "repo": {"full_name": PUBLIC_REPOSITORY}},
            "body": f"Export details\n\n<!-- anyflows-pr-source:v2:17:{source_sha} -->",
        }

        class PrivatePull:
            def request(self, method, path):
                return {"number": 17, "merged": True, "merge_commit_sha": source_sha,
                        "base": {"ref": "develop"}}

        self.assertEqual(SYNC.find_target(PrivatePull(), PRIVATE_REPOSITORY, pr, PUBLIC_REPOSITORY), 17)
        pr["head"]["repo"]["full_name"] = "fork/project"
        self.assertIsNone(SYNC.find_target(PrivatePull(), PRIVATE_REPOSITORY, pr, PUBLIC_REPOSITORY))
        pr["head"]["repo"]["full_name"] = PUBLIC_REPOSITORY
        pr["body"] += "\nUnrelated text"
        self.assertIsNone(SYNC.find_target(PrivatePull(), PRIVATE_REPOSITORY, pr, PUBLIC_REPOSITORY))

    def test_reconcile_updates_adds_and_removes_only_owned_mirrors(self):
        existing = [
            {
                "id": 501,
                "user": {"login": SYNC_USERNAME},
                "body": "Old text\n\n<!-- anyflows-pr-sync-comment:github:42:1 -->",
            },
            {
                "id": 502,
                "user": {"login": SYNC_USERNAME},
                "body": "Deleted source\n\n<!-- anyflows-pr-sync-comment:github:42:2 -->",
            },
            {
                "id": 503,
                "user": {"login": "reviewer"},
                "body": "Private Gitea discussion\n\n<!-- anyflows-pr-sync-comment:github:42:3 -->",
            },
        ]
        source = [
            {
                "id": 1,
                "user": {"login": "alice"},
                "created_at": "2026-10-18T00:00:00Z",
                "html_url": "https://github.com/ChuqiCloud/AnyFlows/pull/42#issuecomment-1",
                "body": "Edited text",
            },
            {
                "id": 3,
                "user": {"login": "bob"},
                "created_at": "2026-10-18T00:01:00Z",
                "html_url": "https://github.com/ChuqiCloud/AnyFlows/pull/42#issuecomment-3",
                "body": "New text",
            },
        ]
        github, gitea = GithubApi(source), GiteaApi(existing)

        SYNC.mirror(github, gitea, PUBLIC_REPOSITORY, PRIVATE_REPOSITORY, SYNC_USERNAME, 42)

        self.assertNotIn(502, gitea.comments)
        self.assertEqual(gitea.comments[501]["body"], SYNC.render_comment(source[0], 42, github_pr()["html_url"]))
        self.assertNotIn("来源评论：", gitea.comments[501]["body"])
        self.assertIn("Private Gitea discussion", gitea.comments[503]["body"])
        self.assertEqual(sum(method == "POST" for method, _, _ in gitea.requests), 1)
        self.assertEqual(sum(method == "PATCH" for method, _, _ in gitea.requests), 1)
        self.assertEqual(sum(method == "DELETE" for method, _, _ in gitea.requests), 1)

        changes = len(gitea.requests)
        SYNC.mirror(github, gitea, PUBLIC_REPOSITORY, PRIVATE_REPOSITORY, SYNC_USERNAME, 42)
        self.assertEqual(len(gitea.requests), changes + 1)  # Authenticated-user check only.


if __name__ == "__main__":
    unittest.main()
