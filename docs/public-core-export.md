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
