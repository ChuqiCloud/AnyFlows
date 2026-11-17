# Live balance review example

This example compares the current balance from a linked external account with its saved snapshots. The user must check the one-time permission in AnyFlows for each review. The host reads the account through its existing connector and stores the new snapshot; the plugin receives only the snapshot fields and a fixed balance projection.

The plugin receives no user ID, account ID, provider URL, credential, request header, or arbitrary HTTP access. It cannot refresh a different account, modify the account binding, credit a wallet, or change the balance ledger. The comparison is informational and is not a provider statement or financial recommendation.

## Package

From the repository root, create a package with:

```sh
python tools/plugins/package-example.py dist/account-balance-live-review.afplugin --example account-balance-live-review
```

The output path must not already exist.

## Install and enable

Upload the package in the administrator's **Plugins** page. Enable it and grant:

- `account.balance.snapshot.live-review.read@1` for the user-selected analysis.
- `external.account.balance.read@1` for the fixed host callback.

The deployment configuration must remain empty. These are distinct grants: the first lets the host invoke the analyzer, and the second lets the plugin request the host's registered balance projection during that invocation. Neither grant provides an account credential or a general network client.

On **Profile and security**, the linked account owner chooses the plugin. The checkbox allowing a current balance read is off by default and applies only to that review. Without the checkbox, AnyFlows invokes the snapshot-only capability; it does not refresh the upstream account. With consent, the host reads the currently selected account, records the returned balance snapshot, then sends the limited projection back through the active Sidecar call.

The review endpoints require a user session or a system PAT with `external_accounts:read`. If a PAT request sets `allow_external_reads` to `true`, it must also have the separate `external_accounts:live_read` scope. The default **Account read-only** PAT preset does not grant this scope; choose **External account live read** only when the token needs it. Per-request consent is still required.

## Python handler

The Python SDK passes a fourth `context` argument only when the handler declares it. Existing three-argument handlers continue to work. A handler can request only a host-registered endpoint identifier:

```python
def review_live_balance(capability_id, public_config, value, context):
    current = context.proxy_read("account.balance")
    # Validate current and value against the published schemas before comparing.
    return make_report(value["snapshots"], current)
```

The callback has no URL, path, headers, body, credential, or account ID. The SDK and host correlate it to the same request, artifact digest, deployment generation, capability grant, and deadline. The host currently accepts at most four callbacks in one invocation and terminates the process on malformed or mismatched protocol frames.

## Extend safely

To expose another provider field, update the host-registered projection and its schema first. Do not add a URL or free-form route parameter to a plugin request. A new remote read needs a separate fixed endpoint, explicit capability grant, per-request user consent, owner check, bounded response projection, and regression coverage.
