# Public core export

The Gitea `develop` tree is the source of truth for the community core. The
GitHub repository is a clean public checkout of a specific Gitea commit; it is
not maintained by copying files by hand.

Run the exporter from a clean Gitea checkout checked out at the commit to be
published:

```sh
python3 tools/repository/export-public-core.py \
  --source . \
  --destination ../AnyFlows-public \
  --commit <gitea-commit> \
  --allow-replace
```

The destination must already be a Git repository with no local changes. The
command checks the enterprise boundary, rejects unapproved sensitive files,
exports the exact commit with `git archive`, and writes
`docs/public-core-source.json` containing the source repository and full
commit id. Legal files listed in `export.preservePaths` remain in the public
repository when they are intentionally maintained there. It never copies the
source working directory or Git credentials.

After reviewing the generated diff, create a branch in the GitHub repository
and open a pull request. The public repository must retain the generated source
metadata so distribution releases can prove which Gitea commit they contain.

The CI-only smoke test exercises the same exporter without compiling the
application:

```sh
bash tools/ci/test-public-core-export.sh
```

## Public contributions

The Gitea `develop` branch is the source of truth. A pull request merged into
the public repository's `main` branch is proposed in Gitea as a reviewable
sync pull request. The workflow applies the patch to a branch from the current
`develop` and does not write directly to the source branch. A maintainer still
reviews and merges the Gitea pull request.

Branches created by the public export process use the
`sync/gitea-public-core-*` prefix and are ignored by the reverse-sync workflow.

The reverse-sync workflow requires the `GITEA_SYNC_USERNAME` and
`GITEA_SYNC_TOKEN` Actions secrets in the GitHub repository. It uses the
merged pull request patch, so it supports merge, squash, and rebase merges.
The generated `docs/public-core-source.json` file is kept under export control
and cannot be changed through a public contribution.

The forward export workflow runs from Gitea on every push to `develop`. It
requires the `PUBLIC_REPO_SYNC_TOKEN` secret in the Gitea repository and creates a
reviewable GitHub PR from the public export. The token is limited to the
`ChuqiCloud/AnyFlows` repository and needs contents and pull request write
access. No GitHub credential is stored in the source tree.
