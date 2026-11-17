# External account balance snapshot review example

This plugin demonstrates a user-selected, read-only analysis of balance snapshots already stored by AnyFlows. It compares recorded amounts by their provider-supplied unit and reports unavailable observations. It does not contact an external platform or change account data.

AnyFlows verifies the signed-in user's ownership of the linked account, loads at most 20 saved snapshots, removes account and provider identifiers, and invokes the selected enabled plugin. The plugin receives only `amount`, `unit`, `status`, and `observedAt`. It never receives the linked account ID, external subject, provider URL, access token, or AnyFlows user ID. The action does not refresh snapshots or decrypt credentials.

The output is an untrusted plugin suggestion, displayed as text for the account owner to review. It is not an accounting record, provider confirmation, or financial recommendation.

## Package

Run from the repository root:

```sh
python tools/plugins/package-example.py dist/account-balance-snapshot-review.afplugin --example account-balance-snapshot-review
```

The output path must not already exist.

## Install and enable

Upload the `.afplugin` in the administrator's **Plugins** page. Enable its deployment and grant the exact capability `account.balance.snapshot.review.read@1`. The deployment configuration is empty.

The account owner can then open **Profile and security**, choose **Plugin analysis** on a linked account, review the data-sharing notice, select this plugin, and press **Analyze**. The host sends the last 20 locally stored balance snapshots. A missing snapshot produces a message instead of an external refresh.

The API is session-authenticated and scoped to the signed-in user's own account:

```text
GET  /api/account/external-accounts/plugin-reviewers
POST /api/account/external-accounts/{id}/plugin-review
```

PAT calls use the existing `external_accounts:read` scope. The plugin itself receives no credentials or API authority.

## Input and output

The plugin receives:

```json
{
  "snapshots": [
    {"amount": "12.3400", "unit": "USD", "status": "available", "observedAt": 1700000000},
    {"amount": null, "unit": null, "status": "unavailable", "observedAt": 1700000300}
  ]
}
```

The host enforces the input and output schemas in `manifest.json`. It rechecks account ownership and the plugin deployment generation after execution. Suggestions are returned in the response; no plugin output is applied to the account, wallet, or balance ledger.

## Modify the example

Keep the input limited to the four snapshot fields. Do not add HTTP requests, credentials, provider URLs, account lookup, or writes. To analyze other account data, define a separate host-projected capability with its own owner check, schema, and explicit consent.
