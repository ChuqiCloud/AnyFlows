#!/usr/bin/env python3
"""Mirror a merged Gitea PR title, description and discussion to GitHub."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener


MIRROR_MARKERS = (
    "<!-- anyflows-pr-sync-comment:",
    "<!-- gitea-source-comment:",
    "<!-- github-source-comment:",
)
EXPORT_BRANCH = "sync/gitea-public-core"
SOURCE_MARKER = "<!-- anyflows-pr-source:v2:"


class SyncError(RuntimeError):
    pass


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        return None


class Api:
    def __init__(self, base, token, scheme):
        self.base = base.rstrip("/")
        self.headers = {
            "Authorization": f"{scheme} {token}",
            "Accept": "application/json",
            "User-Agent": "AnyFlows-PR-sync",
        }
        self.opener = build_opener(NoRedirect())

    def request(self, method, path, body=None):
        data = None if body is None else json.dumps(body, ensure_ascii=False).encode("utf-8")
        request = Request(
            self.base + path,
            data=data,
            method=method,
            headers={**self.headers, "Content-Type": "application/json"},
        )
        try:
            with self.opener.open(request, timeout=30) as response:
                content = response.read()
                return json.loads(content) if content else None
        except HTTPError as error:
            raise SyncError(f"{method} {path} returned HTTP {error.code}") from None
        except (URLError, TimeoutError, ValueError):
            raise SyncError(f"{method} {path} failed or returned invalid JSON") from None

    def pages(self, path, size_parameter, size):
        result = []
        page = 1
        separator = "&" if "?" in path else "?"
        while True:
            batch = self.request("GET", f"{path}{separator}{size_parameter}={size}&page={page}")
            if not isinstance(batch, list):
                raise SyncError(f"{path} did not return a list")
            result.extend(batch)
            if len(batch) < size:
                return result
            page += 1


def source_comment(comment):
    user = comment.get("user") or {}
    body = comment.get("body") or ""
    login = user.get("login") or ""
    return (
        bool(body.strip())
        and user.get("type") != "Bot"
        and not login.endswith("[bot]")
        and not any(marker in body for marker in MIRROR_MARKERS)
    )


def source_id(comment, pr_number, sync_username):
    login = ((comment.get("user") or {}).get("login") or "").casefold()
    if not sync_username or login != sync_username.casefold():
        return None
    pattern = (
        rf"<!-- (?:anyflows-pr-sync-comment:(?:source|gitea)|gitea-source-comment):"
        rf"{pr_number}:([0-9]+) -->\s*\Z"
    )
    match = re.search(pattern, comment.get("body") or "")
    return int(match.group(1)) if match else None


def public_text(value, repository, base_url):
    """Omit configured source locations from text sent to the public repository."""
    value = value or ""
    hostname = urlsplit(base_url).netloc
    if hostname:
        origin = rf"https?://{re.escape(hostname)}(?=[/\s<>()\[\]#?]|$)"
        value = re.sub(rf"\[([^\]]+)\]\({origin}[^)]*\)", r"\1", value, flags=re.IGNORECASE)
        value = re.sub(rf"{origin}[^\s<>()\[\]`]*", "链接已省略", value, flags=re.IGNORECASE)
        value = re.sub(re.escape(hostname), "来源服务", value, flags=re.IGNORECASE)
    if repository:
        value = re.sub(re.escape(repository), "来源仓库", value, flags=re.IGNORECASE)
    return value


def render_comment(comment, pr_number, pr_url, repository=""):
    user = comment.get("user") or {}
    comment_id = comment["id"]
    marker = f"<!-- anyflows-pr-sync-comment:source:{pr_number}:{comment_id} -->"
    created_at = comment.get("created_at") or comment.get("updated_at") or ""
    return (
        f"维护者 `{user.get('login', 'unknown')}`（{created_at}）\n\n"
        f"{public_text(comment['body'], repository, pr_url)}\n\n{marker}"
    )


def source_marker(repository, pr_number, merge_sha):
    return f"{SOURCE_MARKER}{pr_number}:{merge_sha} -->"


def legacy_source_marker(repository, pr_number, merge_sha):
    return f"<!-- anyflows-pr-source:gitea:{repository}:{pr_number}:{merge_sha} -->"


def render_target_body(source_pr, repository, gitea_base_url):
    source_body = public_text(source_pr.get("body"), repository, gitea_base_url)
    merge_sha = source_pr.get("merge_commit_sha") or ""
    number = source_pr.get("number")
    if type(number) is int and number > 0 and re.fullmatch(r"[0-9a-f]{40}", merge_sha):
        return source_body + ("\n\n" if source_body else "") + source_marker(
            repository, number, merge_sha
        )
    return source_body


def find_target(github, public_repository, source_pr, repository):
    if (source_pr.get("base") or {}).get("ref") != "develop":
        return None
    merge_sha = source_pr.get("merge_commit_sha") or ""
    if not source_pr.get("merged") or not re.fullmatch(r"[0-9a-f]{40}", merge_sha):
        return None
    candidates = github.pages(f"/repos/{public_repository}/pulls?state=all", "per_page", 100)
    markers = (
        source_marker(repository, source_pr["number"], merge_sha),
        legacy_source_marker(repository, source_pr["number"], merge_sha),
    )
    matches = [
        candidate for candidate in candidates
        if (candidate.get("base") or {}).get("ref") == "main"
        and (candidate.get("head") or {}).get("ref") == EXPORT_BRANCH
        and ((candidate.get("head") or {}).get("repo") or {}).get("full_name") == public_repository
        and any(re.search(re.escape(marker) + r"\s*\Z", candidate.get("body") or "")
                for marker in markers)
    ]
    if len(matches) > 1:
        raise SyncError("Multiple GitHub PRs match the Gitea source PR")
    return matches[0] if matches else None


def mirror(gitea, github, repository, public_repository, pr_number, sync_metadata):
    prefix = f"/repos/{repository}"
    source_pr = gitea.request("GET", f"{prefix}/pulls/{pr_number}")
    target_pr = find_target(github, public_repository, source_pr, repository)
    if target_pr is None:
        print(f"No GitHub counterpart for Gitea PR #{pr_number}; nothing to mirror")
        return

    target_number = target_pr["number"]
    if sync_metadata:
        body = render_target_body(source_pr, repository, gitea.base.removesuffix("/api/v1"))
        title = public_text(
            source_pr.get("title") or "同步公共核心", repository, gitea.base
        )
        if target_pr.get("title") != title or (target_pr.get("body") or "") != body:
            github.request(
                "PATCH",
                f"/repos/{public_repository}/pulls/{target_number}",
                {"title": title, "body": body},
            )

    account = github.request("GET", "/user")
    sync_username = account.get("login") or ""
    if not sync_username:
        raise SyncError("GitHub sync token did not return an account")

    source_comments = gitea.pages(
        f"{prefix}/issues/{pr_number}/comments", "limit", 50
    )
    desired = {
        comment["id"]: comment
        for comment in source_comments
        if source_comment(comment)
    }
    target_comments = github.pages(
        f"/repos/{public_repository}/issues/{target_number}/comments",
        "per_page",
        100,
    )
    mirrored = {}
    for comment in target_comments:
        identity = source_id(comment, pr_number, sync_username)
        if identity is None:
            continue
        if identity not in desired or identity in mirrored:
            github.request(
                "DELETE",
                f"/repos/{public_repository}/issues/comments/{comment['id']}",
            )
        else:
            mirrored[identity] = comment

    source_url = source_pr.get("html_url") or (
        f"{gitea.base.removesuffix('/api/v1')}/{repository}/pulls/{pr_number}"
    )
    for identity, comment in desired.items():
        body = render_comment(comment, pr_number, source_url, repository)
        previous = mirrored.get(identity)
        if previous is None:
            github.request(
                "POST",
                f"/repos/{public_repository}/issues/{target_number}/comments",
                {"body": body},
            )
        elif previous.get("body") != body:
            github.request(
                "PATCH",
                f"/repos/{public_repository}/issues/comments/{previous['id']}",
                {"body": body},
            )
    print(f"Mirrored Gitea PR #{pr_number} discussion to GitHub PR #{target_number}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pr", type=int)
    parser.add_argument("--render-metadata", type=Path)
    parser.add_argument(
        "--sync-metadata",
        action="store_true",
        help="also update the GitHub PR title and description from Gitea",
    )
    args = parser.parse_args()
    if args.render_metadata:
        if args.pr is not None or args.sync_metadata:
            parser.error("--render-metadata cannot be combined with synchronization options")
        try:
            metadata = json.loads(args.render_metadata.read_text(encoding="utf-8"))
            if not isinstance(metadata, dict):
                raise ValueError("metadata must be an object")
            repository = os.environ["GITEA_REPOSITORY"]
            source_url = os.environ["GITEA_SOURCE_REPOSITORY_URL"]
            source_pr = {
                "number": metadata.get("number"),
                "body": metadata.get("body"),
                "merge_commit_sha": metadata.get("mergeCommitSha"),
            }
            payload = {
                "title": public_text(metadata.get("title") or "同步公共核心", repository, source_url),
                "body": render_target_body(source_pr, repository, source_url),
            }
            print(json.dumps(payload, ensure_ascii=False))
            return 0
        except (KeyError, OSError, ValueError, TypeError):
            print("Public PR metadata could not be prepared", file=sys.stderr)
            return 1
    if args.pr is None or args.pr < 1:
        parser.error("--pr must be positive")
    try:
        repository = os.environ["GITEA_REPOSITORY"]
        public_repository = os.environ["GITHUB_PUBLIC_REPOSITORY"]
        gitea = Api(os.environ["GITEA_API_BASE"], os.environ["GITEA_API_TOKEN"], "token")
        github = Api(
            os.environ.get("GITHUB_API_URL", "https://api.github.com"),
            os.environ["GITHUB_TOKEN"],
            "Bearer",
        )
        mirror(gitea, github, repository, public_repository, args.pr, args.sync_metadata)
    except (KeyError, SyncError) as error:
        print(f"Gitea-to-GitHub PR sync failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
