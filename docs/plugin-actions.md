# Host-confirmed plugin actions

AnyFlows supports one narrow plugin action flow: an administrator can ask an event-analysis plugin to suggest pausing or re-enabling an active channel-monitor target. The plugin submits only a fixed action ID, version, and desired `enabled` boolean. It does not receive the target ID, channel, credential, or model.

The host binds the suggestion to the target in the route, snapshots its configuration version, and stores a five-minute proposal. The review page shows the target and before/after enabled state. A separate administrator confirmation is required. Before applying it, the host checks the operator role again, the plugin artifact approval/revocation, active deployment generation, exact capability grants, target version, and current target state. The target update and proposal outcome commit in one database transaction.

```http
POST /api/admin/channel-monitor/targets/{id}/action-proposals
Content-Type: application/json

{"plugin_id":"org.example.monitor","expected_target_version":7,"idempotency_key":"f3e9ad05-62e1-4801-9a4e-28c03d225c25"}
```

If the selected plugin returns a valid proposal, the host responds with a `proposal_key`, the target ID, previous and requested enabled states, version, status, and expiry. The UI displays that difference and requires an explicit confirmation check.

```http
POST /api/admin/channel-monitor/action-proposals/{proposal_key}/confirm
Content-Type: application/json

{"confirmation_key":"c930c70b-aee9-40fd-9c77-c77db728c02a"}
```

Proposal creation and confirmation require an administrator session or an administrator PAT with the route-specific scopes `admin:propose_admin_channel_monitor_target_action:execute` and `admin:confirm_admin_channel_monitor_action_proposal:execute`. Confirmation retries with the same confirmation key return the saved outcome; conflicting retries fail. Target edits, deletion, deployment replacement, grant removal, approval revocation, or expiry prevent execution.

This is a host action registry, not a general plugin write permission. Sidecar capability invocation still rejects write capabilities. Arbitrary action IDs, target fields, URLs, credentials, SQL, notifications, and financial actions are not accepted. The registered action changes only the monitor target's enabled flag. A later confirmed proposal can re-enable it.

The channel-monitor example includes the additional read-only proposal capability and returns a proposal only when its recorded event state supports a pause or re-enable suggestion. See [the example](../tools/plugins/examples/channel-monitor-review/README.md).
