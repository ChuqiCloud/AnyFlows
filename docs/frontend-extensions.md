# Console extensions

Both built-in frontends discover build-time modules from
`src/extensions/modules/*/index.ts`. The community checkout contains no modules.
Private distributions may assemble modules into this directory before building.

A module exports a `ConsoleExtension` with a unique ID, locale resources and
routes. Routes use `/console/extensions/<name>`, declare `user` or `admin` access
and list every required runtime capability. Duplicate paths and routes outside
this namespace fail at startup. Page imports remain lazy.

Classic uses hash links; Next uses browser routes. Both mount extension pages
inside their existing setup, session and console boundaries. Navigation appears
only when `GET /api/extensions` reports every declared capability. A registered
direct link shows loading, retry or unavailable states when discovery fails or
the capability is absent.

Runtime capabilities describe installed features. Each backend endpoint must
still enforce membership and resource permissions. Modules share the host API
client, query cache and session handling; they must not create a second login
store or put protected data into browser persistence.

The distribution owns the private source checkout, combined OpenAPI client
generation and build process. These files and their generated contracts remain
outside the community repository.

Modules may also declare `loginMethods` with a unique method ID, translated
label, icon and lazy form loader. The host supplies a back action and the
current registration availability. An optional `exchangeTicket` function
returns the host's `LoginResponse` when the method supports a login callback;
single-valued callback parameters other than `ticket`, `error` and `method` are
passed to that method as opaque context.

Browser callbacks use `/#/oauth/callback?method=<module>:<method>&ticket=<ticket>`;
other parameters are passed to the matching method as opaque context.
Next accepts this fragment route as well as its browser callback route. The
host consumes and clears callback parameters before exchanging the ticket,
then establishes the existing management session. Unknown or duplicate methods
are rejected; they never fall back to the built-in OAuth exchange. Tickets
are not added to the module query cache or browser persistence.
