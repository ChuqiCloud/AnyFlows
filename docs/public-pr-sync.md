# Public contribution sync

Pull requests merged into the public repository's `main` branch are proposed
for review in the private Gitea source repository. The workflow creates a
branch from Gitea `develop`, applies the public change set, pushes the branch,
and opens a pull request. It does not modify `develop` directly.

The generated public export branches use the `sync/gitea-public-core-*` prefix
and are excluded from this workflow to prevent a synchronization loop.

The GitHub repository must define these Actions secrets:

- `GITEA_SYNC_USERNAME`: Gitea account used by the sync bot.
- `GITEA_SYNC_TOKEN`: Gitea token with permission to read the repository, push
  branches, and create pull requests.

If the public change set cannot be applied cleanly to `develop`, the workflow
fails without creating a branch or pull request. Resolve the change against
the current Gitea source and submit it through the normal Gitea review flow.
