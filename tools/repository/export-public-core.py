#!/usr/bin/env python3
"""Export an exact public-core commit into a clean repository checkout.

The command is intentionally source-controlled so the Gitea source tree is the
authority for the export rules. It copies a Git archive, never the source
working directory, and writes deterministic source metadata for release tools.
"""

from __future__ import annotations

import argparse
from difflib import SequenceMatcher
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile
from typing import Sequence


class ExportError(RuntimeError):
    """An input failed an export safety check."""


def run_git(repository: Path, arguments: Sequence[str]) -> str:
    result = subprocess.run(
        ["git", "-C", str(repository), *arguments],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ExportError(f"git {' '.join(arguments)} failed: {detail}")
    return result.stdout.strip()


def ensure_clean(repository: Path, label: str) -> None:
    status = run_git(repository, ["status", "--porcelain=v1", "--untracked-files=all"])
    if status:
        raise ExportError(f"{label} has uncommitted or untracked files")


def resolve_commit(source: Path, requested: str) -> str:
    commit = run_git(source, ["rev-parse", "--verify", f"{requested}^{{commit}}"])
    if len(commit) != 40:
        raise ExportError("source commit must resolve to a full Git object id")
    return commit


def load_manifest(source: Path, manifest_path: Path) -> dict:
    try:
        value = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ExportError(f"cannot read boundary manifest: {error}") from error
    if not isinstance(value, dict):
        raise ExportError("boundary manifest must be a JSON object")
    return value


def run_boundary_check(source: Path, manifest_path: Path) -> None:
    verifier = source / "tools" / "ci" / "verify-public-core-boundary.py"
    if not verifier.is_file():
        raise ExportError(f"boundary verifier is missing: {verifier}")
    result = subprocess.run(
        [
            sys.executable,
            str(verifier),
            "--root",
            str(source),
            "--manifest",
            str(manifest_path),
            "--mode",
            "public",
        ],
        check=False,
    )
    if result.returncode:
        raise ExportError("public-core boundary check failed")


def tracked_paths(source: Path, commit: str) -> list[str]:
    raw = run_git(source, ["ls-tree", "-r", "--name-only", commit])
    paths = [line for line in raw.splitlines() if line]
    if not paths:
        raise ExportError("source commit contains no files")
    return paths


def validate_sensitive_paths(paths: Sequence[str], export_config: dict) -> None:
    patterns = export_config.get("sensitiveFilePatterns", [])
    allowed = set(export_config.get("allowedTestFixtures", []))
    if not isinstance(patterns, list) or not all(isinstance(item, str) for item in patterns):
        raise ExportError("export.sensitiveFilePatterns must be an array of strings")
    if not isinstance(export_config.get("allowedTestFixtures", []), list) or not all(
        isinstance(item, str) for item in export_config.get("allowedTestFixtures", [])
    ):
        raise ExportError("export.allowedTestFixtures must be an array of strings")

    blocked = [
        path
        for path in paths
        if any(PurePosixPath(path).match(pattern) for pattern in patterns) and path not in allowed
    ]
    if blocked:
        joined = ", ".join(blocked)
        raise ExportError(f"sensitive files are not allowlisted for export: {joined}")


def safe_member_path(destination: Path, name: str) -> Path:
    relative = PurePosixPath(name)
    if relative.is_absolute() or ".." in relative.parts:
        raise ExportError(f"archive contains an unsafe path: {name}")
    target = destination.joinpath(*relative.parts)
    try:
        target.relative_to(destination)
    except ValueError as error:
        raise ExportError(f"archive contains an unsafe path: {name}") from error
    return target


def remove_tracked_files(destination: Path, preserve_paths: set[str]) -> None:
    for path in run_git(destination, ["ls-files", "-z"]).split("\x00"):
        if not path:
            continue
        if path in preserve_paths:
            continue
        target = safe_member_path(destination, path)
        if target.is_file() or target.is_symlink():
            target.unlink()
        elif target.is_dir():
            shutil.rmtree(target)
    for directory in sorted(
        (path for path in destination.rglob("*") if path.is_dir() and ".git" not in path.parts),
        key=lambda path: len(path.parts),
        reverse=True,
    ):
        try:
            directory.rmdir()
        except OSError:
            pass


def export_archive(source: Path, destination: Path, commit: str) -> None:
    process = subprocess.Popen(
        ["git", "-C", str(source), "archive", "--format=tar", "--worktree-attributes", commit],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdout is not None
    try:
        with tarfile.open(fileobj=process.stdout, mode="r|") as archive:
            for member in archive:
                target = safe_member_path(destination, member.name)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                if not member.isfile():
                    raise ExportError(f"archive contains unsupported entry: {member.name}")
                target.parent.mkdir(parents=True, exist_ok=True)
                source_file = archive.extractfile(member)
                if source_file is None:
                    raise ExportError(f"cannot read archive entry: {member.name}")
                with target.open("wb") as output:
                    shutil.copyfileobj(source_file, output)
                target.chmod(member.mode & 0o777)
    finally:
        if process.stdout:
            process.stdout.close()
    stderr = process.stderr.read().decode("utf-8", errors="replace") if process.stderr else ""
    return_code = process.wait()
    if return_code:
        raise ExportError(f"git archive failed: {stderr.strip()}")


def write_metadata(destination: Path, relative_path: str, repository: str, commit: str) -> None:
    target = safe_member_path(destination, relative_path)
    target.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schemaVersion": 1,
        "sourceRepository": repository,
        "sourceCommit": commit,
    }
    target.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def tree_entry(repository: Path, revision: str, path: str) -> tuple[str, str] | None:
    result = subprocess.run(
        ["git", "-C", str(repository), "ls-tree", "-z", "--full-tree", revision, "--", f":(literal){path}"],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ExportError(f"cannot read tree entry for {path}: {detail}")
    records = [record for record in result.stdout.split(b"\x00") if record]
    if not records:
        return None
    header, separator, _ = records[0].partition(b"\t")
    if not separator:
        raise ExportError(f"cannot parse tree entry for {path}")
    mode, _object_type, object_id = header.decode("ascii").split(" ")
    return mode, object_id


def blob_content(repository: Path, object_id: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(repository), "cat-file", "blob", object_id],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ExportError(f"cannot read Git blob {object_id}: {detail}")
    return result.stdout


def changed_line_chunks(base_content: bytes, public_content: bytes) -> tuple[list[list[bytes]], list[tuple[int, int]]]:
    base_lines = base_content.splitlines()
    public_lines = public_content.splitlines()
    matcher = SequenceMatcher(a=base_lines, b=public_lines, autojunk=False)
    added: list[list[bytes]] = []
    removed: list[tuple[int, int]] = []
    for tag, base_start, base_end, public_start, public_end in matcher.get_opcodes():
        if tag in {"replace", "insert"} and public_start != public_end:
            added.append(public_lines[public_start:public_end])
        if tag in {"replace", "delete"} and base_start != base_end:
            removed.append((base_start, base_end))
    return added, removed


def contains_chunks_in_order(source_lines: list[bytes], chunks: list[list[bytes]]) -> bool:
    cursor = 0
    for chunk in chunks:
        for line in chunk:
            position = next(
                (
                    index
                    for index in range(cursor, len(source_lines))
                    if source_lines[index] == line
                ),
                None,
            )
            if position is None:
                return False
            cursor = position + 1
    return True


def matched_base_lines(base_content: bytes, source_content: bytes) -> set[int]:
    base_lines = base_content.splitlines()
    source_lines = source_content.splitlines()
    matched: set[int] = set()
    matcher = SequenceMatcher(a=base_lines, b=source_lines, autojunk=False)
    for tag, base_start, base_end, _source_start, _source_end in matcher.get_opcodes():
        if tag == "equal":
            matched.update(
                index for index in range(base_start, base_end) if base_lines[index].strip()
            )
    return matched


def public_changes_are_reconciled(
    repository: Path, base: str, source_tree: str, public_tree: str
) -> tuple[bool, list[str]]:
    result = subprocess.run(
        ["git", "-C", str(repository), "diff", "--no-renames", "--name-only", "-z", base, public_tree, "--"],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise ExportError(f"cannot list public changes: {detail}")

    missing: list[str] = []
    for raw_path in result.stdout.split(b"\x00"):
        if not raw_path:
            continue
        path = raw_path.decode("utf-8")
        source_entry = tree_entry(repository, source_tree, path)
        public_entry = tree_entry(repository, public_tree, path)
        if source_entry == public_entry:
            continue
        if source_entry is None or public_entry is None:
            missing.append(path)
            continue

        base_entry = tree_entry(repository, base, path)
        if base_entry is None:
            missing.append(path)
            continue
        source_mode, source_blob = source_entry
        public_mode, public_blob = public_entry
        if source_mode != public_mode or source_mode not in {"100644", "100755"}:
            missing.append(path)
            continue
        _base_mode, base_blob = base_entry
        base_content = blob_content(repository, base_blob)
        source_content = blob_content(repository, source_blob)
        public_content = blob_content(repository, public_blob)
        if source_content == public_content:
            continue
        additions, removals = changed_line_chunks(base_content, public_content)
        source_lines = source_content.splitlines()
        if not contains_chunks_in_order(source_lines, additions):
            missing.append(path)
            continue

        matched = matched_base_lines(base_content, source_content)
        if any(index in matched for start, end in removals for index in range(start, end)):
            missing.append(path)

    return not missing, missing


def verify_public_sync(
    source: Path, destination: Path, commit: str, metadata_path: str, preserve_paths: set[str]
) -> None:
    """Reject exports that would discard public changes since the last export.

    Merge synthetic snapshots in a temporary repository. This compares content
    across independent histories and leaves both input repositories untouched.
    Legal files maintained in the public repository and generated provenance
    do not participate in the comparison.
    """
    ignored = sorted(preserve_paths | {metadata_path})
    public_paths = set(run_git(destination, ["ls-files", "-z"]).split("\x00")) - {""}
    if metadata_path not in public_paths:
        if public_paths - preserve_paths:
            raise ExportError(
                "public repository has no export provenance; reconcile its contents before exporting"
            )
        return
    anchor = run_git(destination, ["log", "-1", "--format=%H", "HEAD", "--", metadata_path])
    if not anchor:
        raise ExportError("cannot locate the public export checkpoint")
    public_head = resolve_commit(destination, "HEAD")
    with tempfile.TemporaryDirectory(prefix="anyflows-public-sync-") as directory:
        scratch = Path(directory)
        run_git(scratch, ["init", "--quiet"])
        run_git(scratch, ["fetch", "--quiet", "--no-tags", "--depth=1", str(source), commit])
        run_git(
            scratch,
            ["fetch", "--quiet", "--no-tags", "--depth=1", str(destination), anchor, public_head],
        )
        # These identities only label temporary commits; they are never pushed.
        run_git(scratch, ["config", "user.name", "Public sync check"])
        run_git(scratch, ["config", "user.email", "public-sync@example.invalid"])

        def snapshot(revision: str) -> str:
            run_git(scratch, ["read-tree", revision])
            run_git(scratch, ["update-index", "--force-remove", "--", *ignored])
            return run_git(scratch, ["write-tree"])

        base_tree = snapshot(anchor)
        source_tree = snapshot(commit)
        public_tree = snapshot(public_head)
        base = run_git(scratch, ["commit-tree", base_tree, "-m", "export checkpoint"])
        left = run_git(scratch, ["commit-tree", source_tree, "-p", base, "-m", "source"])
        right = run_git(scratch, ["commit-tree", public_tree, "-p", base, "-m", "public"])

        result = subprocess.run(
            ["git", "-C", str(scratch), "merge-tree", "--write-tree", left, right],
            check=False, capture_output=True, text=True, encoding="utf-8",
        )
        if result.returncode not in (0, 1):
            raise ExportError("public sync comparison failed; Git 2.38 or newer is required")
        merged_tree = result.stdout.splitlines()[0] if result.stdout else ""
        if result.returncode or merged_tree != source_tree:
            reconciled, missing = public_changes_are_reconciled(
                scratch, base, source_tree, public_tree
            )
            if reconciled:
                return
            preview = ", ".join(missing[:20])
            raise ExportError(
                "public changes have not been reconciled into the source: " + preview
                + "; merge the missing GitHub contributions into Gitea before exporting"
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path.cwd())
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--commit", default="HEAD", help="source commit or ref to export")
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("docs/public-core-extraction.json"),
    )
    parser.add_argument(
        "--allow-replace",
        action="store_true",
        help="allow replacement of tracked files in the destination repository",
    )
    args = parser.parse_args()
    source = args.source.resolve()
    destination = args.destination.resolve()
    manifest_path = args.manifest if args.manifest.is_absolute() else source / args.manifest

    try:
        if not source.is_dir() or not destination.is_dir():
            raise ExportError("source and destination must be existing directories")
        if source == destination:
            raise ExportError("source and destination must be different repositories")
        if os.path.commonpath([source, destination]) == str(source):
            raise ExportError("destination must not be inside the source repository")
        ensure_clean(source, "source repository")
        ensure_clean(destination, "destination repository")
        if not args.allow_replace:
            raise ExportError("pass --allow-replace after reviewing the destination repository")

        commit = resolve_commit(source, args.commit)
        current_head = run_git(source, ["rev-parse", "HEAD"])
        if commit != current_head:
            raise ExportError(
                "source checkout must be at the requested commit; use a separate worktree for older revisions"
            )
        manifest = load_manifest(source, manifest_path)
        run_boundary_check(source, manifest_path)
        paths = tracked_paths(source, commit)
        export_config = manifest.get("export", {})
        if not isinstance(export_config, dict):
            raise ExportError("boundary manifest export must be an object")
        preserve_paths_value = export_config.get("preservePaths", [])
        if not isinstance(preserve_paths_value, list) or not all(
            isinstance(item, str) and item for item in preserve_paths_value
        ):
            raise ExportError("export.preservePaths must be an array of non-empty strings")
        preserve_paths = set(preserve_paths_value)
        for path in preserve_paths:
            safe_member_path(destination, path)
        validate_sensitive_paths(paths, export_config)
        metadata_path = export_config.get("metadataPath", "docs/public-core-source.json")
        repository = manifest.get("publicRepository")
        if not isinstance(metadata_path, str) or not metadata_path:
            raise ExportError("export.metadataPath must be a non-empty string")
        if not isinstance(repository, str) or not repository:
            raise ExportError("publicRepository must be a non-empty string")

        verify_public_sync(source, destination, commit, metadata_path, preserve_paths)
        remove_tracked_files(destination, preserve_paths)
        export_archive(source, destination, commit)
        write_metadata(destination, metadata_path, repository, commit)
        print(f"exported public core {commit} to {destination}")
        print(f"source metadata: {metadata_path}")
        return 0
    except ExportError as error:
        print(f"public-core export failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
