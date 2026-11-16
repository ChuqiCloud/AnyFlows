# Merchant service event example

This example handles public service publication, revision update, pause, and restore events. AnyFlows delivers these events from the same database transaction that commits the state change.

The plugin receives a stable `eventKey`, `serviceId`, the published `revision`, `status` (`active` or `paused`), and `observedAt`. It does not receive the merchant identity, service description, model list, prices, upstream URL, or credentials. The example produces a short report; it does not modify the service or notify users.

Delivery is at least once: a worker retry reuses the same `eventKey`. Use this value as the idempotency key at any durable destination before adding a side effect. The current read-only handler is safe to run more than once.

## Package

Run from the AnyFlows repository root:

```sh
python tools/plugins/package-example.py dist/merchant-service-event.afplugin --example merchant-service-event
```

The output path must not already exist.

## Install and enable

Upload the `.afplugin` file in the administrator's **Plugins** page. A normal upload registers the artifact. Manually registered artifacts must be approved first. Then enable the deployment and grant `events.merchant-service.changed.consume@1` in its advanced settings.

Only enabled, approved, unrevoked deployments with the exact capability grant receive events. Delivery uses the shared persistent queue, bounded retries, and generation checks. Results are available only to administrators through the plugin event results page and API. PAT access requires `admin:list_admin_plugin_event_results:execute`.

## Extend the example

Keep the fixed input and output schemas. Add only logic that can work from the five public fields. Do not add HTTP requests, shell commands, merchant lookups, or writes; new event data requires a host-owned schema and privacy review first.
