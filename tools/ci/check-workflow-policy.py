#!/usr/bin/env python3
"""Check CI lifecycle rules without starting application builds (PyYAML 6.x)."""

from pathlib import Path
import sys

import yaml


ROOT = Path(__file__).resolve().parents[2]
VALIDATION_WORKFLOWS = (
    "ci.yml",
    "cache.yml",
    "console-extensions.yml",
    "conversation-compatibility.yml",
    "database-extension.yml",
    "frontend-generated-assets.yml",
    "proxy-pools.yml",
    "public-sync-checks.yml",
    "release-checks.yml",
)
SERIAL_WORKFLOWS = (
    "export-public-core.yml",
    "mirror-gitea-pr-discussion.yml",
    "propose-gitea-sync.yml",
    "publish-public-release.yml",
)


def expression(value):
    return str(value or "").removeprefix("${{").removesuffix("}}").strip()


def main():
    errors = []
    workflows = {}
    for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
        # BaseLoader preserves GitHub's `on` key rather than interpreting YAML 1.1 booleans.
        workflows[path.name] = yaml.load(path.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)

    def require(condition, filename, message):
        if not condition:
            errors.append(f"{filename}: {message}")

    for filename in VALIDATION_WORKFLOWS:
        workflow = workflows[filename]
        triggers = workflow["on"]
        pull_request = triggers["pull_request"]
        require(
            {"opened", "synchronize", "reopened", "closed"}.issubset(pull_request.get("types", [])),
            filename,
            "PR updates and closure must trigger lifecycle evaluation",
        )
        concurrency = workflow.get("concurrency", {})
        require(concurrency.get("cancel-in-progress") == "true", filename, "superseded checks must cancel")
        group = concurrency.get("group", "")
        require(
            "github.event.pull_request.number || github.ref" in group,
            filename,
            "use a stable PR group across update and closed events",
        )
        push = triggers.get("push") or {}
        require(
            set(push.get("branches", [])).issubset({"main", "develop", "master"}),
            filename,
            "feature checks belong to PR events; avoid duplicate push triggers",
        )
        jobs = workflow["jobs"]
        for name, job in jobs.items():
            condition = expression(job.get("if"))
            guarded = condition == "github.event.action != 'closed'"
            # CI's downstream jobs inherit the skipped path detector. Status functions
            # would override this default and could allocate runners after closure.
            needs = job.get("needs", [])
            if isinstance(needs, str):
                needs = [needs]
            inherited = bool(needs) and all(
                expression(jobs[need].get("if")) == "github.event.action != 'closed'"
                for need in needs
            ) and not any(marker in condition for marker in ("always(", "failure(", "cancelled("))
            require(guarded or inherited, filename, f"{name} could allocate a runner on PR closure")
            timeout = int(job.get("timeout-minutes", "0"))
            require(0 < timeout <= 60, filename, f"{name} needs a bounded validation timeout")

    for filename in SERIAL_WORKFLOWS:
        require(
            workflows[filename]["concurrency"]["cancel-in-progress"] == "false",
            filename,
            "publishing and synchronization must finish serially",
        )

    for filename in ("database-extension.yml", "cache.yml", "console-extensions.yml", "public-sync-checks.yml"):
        require(
            {"main", "develop"}.issubset(workflows[filename]["on"]["pull_request"]["branches"]),
            filename,
            "PR validation must cover main and develop regardless of source branch name",
        )

    public = workflows["public-sync-checks.yml"]
    require(
        "head.repo" not in str(public["jobs"]["export"].get("if", "")),
        "public-sync-checks.yml",
        "same-repository PRs must not be skipped when feature push checks are disabled",
    )
    database_steps = workflows["database-extension.yml"]["jobs"]["migration-history"]["steps"]
    require(
        any(str(step.get("uses", "")).startswith("Swatinem/rust-cache@") for step in database_steps),
        "database-extension.yml",
        "database checks must reuse Rust build artifacts",
    )
    cache = workflows["cache.yml"]["jobs"]["cache"]
    require(
        cache["services"]["redis"].get("ports") == ["6379/tcp"],
        "cache.yml",
        "use a dynamic Redis host port to avoid parallel job conflicts",
    )
    require(
        any(str(step.get("uses", "")).startswith("Swatinem/rust-cache@") for step in cache["steps"]),
        "cache.yml",
        "cache contract checks must reuse Rust build artifacts",
    )
    require(
        workflows["verify-preview-opencode.yml"].get("concurrency", {}).get("cancel-in-progress") == "true",
        "verify-preview-opencode.yml",
        "repeated acceptance checks must cancel the previous run",
    )

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"CI lifecycle policy passed for {len(VALIDATION_WORKFLOWS)} validation workflows.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
