"""Regression checks for Gitea-to-GitHub PR metadata and discussion mirroring."""

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import unittest


SPEC = spec_from_file_location(
    "gitea_pr_comment_sync", Path(__file__).parents[1] / "sync-gitea-pr-comments.py"
)
assert SPEC and SPEC.loader
SYNC = module_from_spec(SPEC)
SPEC.loader.exec_module(SYNC)


REPOSITORY = "source/project"
PUBLIC_REPOSITORY = "ChuqiCloud/AnyFlows"
MERGE_SHA = "b" * 40


def source_pr():
    return {
        "number": 17,
        "title": "修复同步流程",
        "body": "修复说明",
        "html_url": "https://example.test/source/project/pulls/17",
        "merged": True,
        "merge_commit_sha": MERGE_SHA,
        "base": {"ref": "develop"},
    }


class GiteaApi:
    base = "https://example.test/api/v1"

    def __init__(self, comments):
        self.source = source_pr()
        self.comments = {comment["id"]: dict(comment) for comment in comments}

    def request(self, method, path, body=None):
        if method == "GET" and path == f"/repos/{REPOSITORY}/pulls/17":
            return self.source
        raise AssertionError(f"Unexpected Gitea request: {method} {path}")

    def pages(self, path, size_parameter, size):
        if path == f"/repos/{REPOSITORY}/issues/17/comments":
            return list(self.comments.values())
        raise AssertionError(f"Unexpected Gitea page request: {path}")


class GithubApi:
    def __init__(self, comments):
        self.comments = {comment["id"]: dict(comment) for comment in comments}
        self.requests = []
        self.next_id = max(self.comments, default=700) + 1
        self.target = {
            "number": 55,
            "title": "旧标题",
            "body": SYNC.render_target_body(source_pr(), REPOSITORY, "https://example.test"),
            "head": {"ref": SYNC.EXPORT_BRANCH, "repo": {"full_name": PUBLIC_REPOSITORY}},
            "base": {"ref": "main"},
        }

    def request(self, method, path, body=None):
        self.requests.append((method, path, body))
        if method == "GET" and path == "/user":
            return {"login": "sync-bot"}
        if method == "PATCH" and path == f"/repos/{PUBLIC_REPOSITORY}/pulls/55":
            self.target.update(body)
            return self.target
        if method == "DELETE" and path.startswith(
            f"/repos/{PUBLIC_REPOSITORY}/issues/comments/"
        ):
            self.comments.pop(int(path.rsplit("/", 1)[1]), None)
            return None
        if method == "PATCH" and path.startswith(
            f"/repos/{PUBLIC_REPOSITORY}/issues/comments/"
        ):
            comment = self.comments[int(path.rsplit("/", 1)[1])]
            comment.update(body)
            return comment
        if method == "POST" and path == f"/repos/{PUBLIC_REPOSITORY}/issues/55/comments":
            comment = {"id": self.next_id, "user": {"login": "sync-bot"}, **body}
            self.next_id += 1
            self.comments[comment["id"]] = comment
            return comment
        raise AssertionError(f"Unexpected GitHub request: {method} {path}")

    def pages(self, path, size_parameter, size):
        if path.startswith(f"/repos/{PUBLIC_REPOSITORY}/pulls?"):
            return [self.target]
        if path == f"/repos/{PUBLIC_REPOSITORY}/issues/55/comments":
            return list(self.comments.values())
        raise AssertionError(f"Unexpected GitHub page request: {path}")


class GiteaToGithubSyncTests(unittest.TestCase):
    def test_source_filter_ignores_bots_and_existing_mirrors(self):
        self.assertTrue(SYNC.source_comment({"user": {"login": "alice"}, "body": "Review"}))
        self.assertFalse(SYNC.source_comment({"user": {"type": "Bot"}, "body": "Build"}))
        self.assertFalse(SYNC.source_comment({"user": {"login": "sync[bot]"}, "body": "Build"}))
        self.assertFalse(SYNC.source_comment({
            "user": {"login": "alice"},
            "body": "copy\n\n<!-- anyflows-pr-sync-comment:github:42:12 -->",
        }))

    def test_target_requires_exact_source_marker(self):
        github = GithubApi([])
        source = source_pr()
        self.assertEqual(
            SYNC.find_target(github, PUBLIC_REPOSITORY, source, REPOSITORY)["number"], 55
        )
        source["merge_commit_sha"] = "c" * 40
        self.assertIsNone(SYNC.find_target(github, PUBLIC_REPOSITORY, source, REPOSITORY))
        github.target["head"]["repo"]["full_name"] = "fork/project"
        self.assertIsNone(SYNC.find_target(github, PUBLIC_REPOSITORY, source_pr(), REPOSITORY))

    def test_legacy_target_marker_remains_readable_and_is_replaced_on_update(self):
        github = GithubApi([])
        github.target["body"] = "修复说明\n\n" + SYNC.legacy_source_marker(REPOSITORY, 17, MERGE_SHA)
        self.assertEqual(
            SYNC.find_target(github, PUBLIC_REPOSITORY, source_pr(), REPOSITORY)["number"], 55
        )
        SYNC.mirror(GiteaApi([]), github, REPOSITORY, PUBLIC_REPOSITORY, 17, True)
        self.assertIn("<!-- anyflows-pr-source:v2:17:", github.target["body"])
        self.assertNotIn(REPOSITORY, github.target["body"])
        self.assertNotIn("anyflows-pr-source:gitea:", github.target["body"])

    def test_source_marker_must_be_trailing_and_belong_to_a_merged_develop_pr(self):
        for override in ({"merged": False}, {"base": {"ref": "main"}},
                         {"merge_commit_sha": "invalid"}):
            self.assertIsNone(SYNC.find_target(
                GithubApi([]), PUBLIC_REPOSITORY, {**source_pr(), **override}, REPOSITORY
            ))
        github = GithubApi([])
        github.target["body"] += "\nUnrelated text"
        self.assertIsNone(SYNC.find_target(github, PUBLIC_REPOSITORY, source_pr(), REPOSITORY))

    def test_target_body_keeps_only_source_text_and_hidden_mapping_marker(self):
        body = SYNC.render_target_body(source_pr(), REPOSITORY, "https://example.test")
        self.assertEqual(
            body,
            "修复说明\n\n<!-- anyflows-pr-source:v2:17:" + MERGE_SHA + " -->",
        )
        for visible_note in ("来源 PR：", "来源 PR 编号：", "来源提交：", "来源仓库："):
            self.assertNotIn(visible_note, body)

    def test_public_output_omits_source_locations_and_keeps_link_labels(self):
        text = "参见 [变更](https://example.test/source/project/pulls/17)。\nhttps://example.test/source/project/issues/2\nsource/project example.test"
        source = {**source_pr(), "body": text}
        body = SYNC.render_target_body(source, REPOSITORY, "https://example.test")
        self.assertIn("参见 变更。", body)
        self.assertNotIn("example.test", body)
        self.assertNotIn(REPOSITORY, body)
        comment = {"id": 9, "body": text, "user": {"login": "alice"}, "created_at": "2026-10-24"}
        rendered = SYNC.render_comment(comment, 17, source_pr()["html_url"], REPOSITORY)
        self.assertIn("维护者 `alice`", rendered)
        self.assertIn("<!-- anyflows-pr-sync-comment:source:17:9 -->", rendered)
        for private_value in ("Gitea", "example.test", REPOSITORY):
            self.assertNotIn(private_value, rendered)
        mixed_case = SYNC.public_text("https://EXAMPLE.TEST/Source/Project/pulls/17 Source/Project", REPOSITORY, "https://example.test")
        self.assertNotIn("example.test", mixed_case.casefold())
        self.assertNotIn(REPOSITORY, mixed_case.casefold())

    def test_source_comment_markers_read_old_and_new_formats(self):
        for prefix in ("anyflows-pr-sync-comment:source", "anyflows-pr-sync-comment:gitea", "gitea-source-comment"):
            comment = {"user": {"login": "sync-bot"}, "body": f"copy\n\n<!-- {prefix}:17:9 -->"}
            self.assertEqual(SYNC.source_id(comment, 17, "sync-bot"), 9)
            self.assertIsNone(SYNC.source_id(comment, 17, "reviewer"))

    def test_reconcile_updates_adds_and_removes_only_owned_mirrors(self):
        source_comments = [
            {
                "id": 1,
                "user": {"login": "alice"},
                "created_at": "2026-10-19T00:00:00Z",
                "html_url": "https://example.test/comment/1",
                "body": "已编辑",
            },
            {
                "id": 3,
                "user": {"login": "bob"},
                "created_at": "2026-10-19T00:01:00Z",
                "html_url": "https://example.test/comment/3",
                "body": "新增讨论",
            },
        ]
        target_comments = [
            {
                "id": 501,
                "user": {"login": "sync-bot"},
                "body": "旧内容\n\n<!-- anyflows-pr-sync-comment:gitea:17:1 -->",
            },
            {
                "id": 502,
                "user": {"login": "sync-bot"},
                "body": "已删除\n\n<!-- anyflows-pr-sync-comment:gitea:17:2 -->",
            },
            {
                "id": 503,
                "user": {"login": "reviewer"},
                "body": "GitHub 原生讨论",
            },
        ]
        gitea = GiteaApi(source_comments)
        github = GithubApi(target_comments)

        SYNC.mirror(gitea, github, REPOSITORY, PUBLIC_REPOSITORY, 17, True)

        self.assertEqual(github.target["title"], source_pr()["title"])
        self.assertEqual(github.target["body"], SYNC.render_target_body(source_pr(), REPOSITORY, "https://example.test"))
        self.assertNotIn(502, github.comments)
        self.assertEqual(
            github.comments[501]["body"],
            SYNC.render_comment(source_comments[0], 17, source_pr()["html_url"]),
        )
        self.assertNotIn("来源评论：", github.comments[501]["body"])
        self.assertIn("GitHub 原生讨论", github.comments[503]["body"])
        self.assertEqual(sum(method == "POST" for method, _, _ in github.requests), 1)
        self.assertEqual(sum(method == "PATCH" for method, _, _ in github.requests), 2)
        self.assertEqual(sum(method == "DELETE" for method, _, _ in github.requests), 1)

        changes = len(github.requests)
        SYNC.mirror(gitea, github, REPOSITORY, PUBLIC_REPOSITORY, 17, True)
        self.assertEqual(len(github.requests), changes + 1)  # Authenticated-user check only.


if __name__ == "__main__":
    unittest.main()
