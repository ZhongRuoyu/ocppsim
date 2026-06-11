# Design

`ocppsim` is organized around a stateful simulator event loop.
The event loop bridges three streams of work:

- user commands from the terminal UI,
- outbound OCPP CALL messages queued by simulator workflows,
- inbound WebSocket frames from the CSMS.

## Runtime Model

The Tokio runtime owns one `Simulator` value for the active session.
The simulator stores protocol-neutral operating state, pending CALL context,
and an outbound queue.
Feature-specific state, such as reservations, charging profiles, certificates,
and security events, lives beside the connector and transaction state so the
same event loop can coordinate cross-cutting workflows.

When connected, the loop sends the next queued CALL only when there is no
pending request.
The outbound queue is capped by `outbound-queue-limit` unless that value is `0`.
The pending context records what response is expected and how to update local
state when a CALLRESULT or CALLERROR arrives.
Request timeouts clear stale pending state so later queued messages can move.
Start and stop transaction requests keep enough pending context to roll back or
finish local transaction state when the CSMS rejects, times out, or accepts the
request.

The WebSocket handshake requires the CSMS to negotiate the configured OCPP-J
subprotocol token.
A missing or mismatched `Sec-WebSocket-Protocol` response fails the connection
instead of silently continuing with an ambiguous protocol version.
See [`ocpp-support.md`](ocpp-support.md) for the exact support behavior behind
these runtime paths.

## Protocol Dispatch

Inbound CALL frames are parsed into OCPP-J frame types, then dispatched by the
configured protocol version.
The WebSocket reader caps inbound messages at 1 MiB, and the OCPP-J frame parser
enforces the specification's 36-character `messageId` limit before dispatch.
OCPP 1.6 and OCPP 2.x have separate dispatch modules because action names,
payload shapes, and response statuses differ.
Typed request extractors stay side-effect free until required fields have been
validated, which lets handlers reject malformed supported requests without
partial state changes.
Duplicate-message handling, strict validation, and action support are support
semantics documented in [`ocpp-support.md`](ocpp-support.md).

## State And Workflows

The simulator keeps protocol-neutral state where possible.
Outbound workflow methods translate that state into version-specific OCPP
payloads at enqueue time.
OCPP 1.6 uses `StartTransaction` and `StopTransaction`; OCPP 2.0.1 and OCPP
2.1 use `TransactionEvent`.
Transaction workflows keep enough pending context to roll back, finish, or
advance local state after the corresponding CSMS response, error, or timeout.

`ChangeAvailability` requests that make an active connector inoperative are
stored as scheduled changes so connector transitions can be resolved after the
active transaction finishes.
Availability changes and post-transaction cleanup share connector transition
helpers, which keeps reservations and scheduled availability coordinated.

Configuration is stored as a map of OCPP 1.6-style keys.
OCPP 1.6 exposes that map through `GetConfiguration` and
`ChangeConfiguration`.
OCPP 2.0.1 and OCPP 2.1 expose the same backing values through
`GetVariables` and `SetVariables` for component `ChargingStation` or
`SecurityCtrlr`.
The mapping table and unsupported variable scope live in
[`ocpp-support.md`](ocpp-support.md).

Accepted `BootNotification` responses update the local `HeartbeatInterval`
configuration value and start or restart periodic heartbeats with the interval
returned by the CSMS.

Smart charging stores one effective profile per connector.
The simulator applies the first supported limit value to connector state and
composite schedules.
Support limits for starts, boot gating, reservations, smart charging, and
composite schedules are documented in [`ocpp-support.md`](ocpp-support.md).

Each local connector is modeled as one OCPP 2.x EVSE with `connectorId = 1`.
This keeps 1.6 connector addressing and 2.x EVSE addressing aligned for the
current simulator scope.
Supporting multiple physical connectors per EVSE would require a richer EVSE
model and separate connector state beneath each EVSE.

Security extension behavior is modeled at the simulator boundary.
When CA or client certificate paths are provided, the connection uses a custom
rustls connector with WebPKI roots plus the configured PEM files.
Configured CA and client certificate/key paths are treated as the OCPP-level
certificate prerequisites for security-profile upgrades, because those files
are the real transport trust and identity material used by rustls.
Certificate-management actions maintain deterministic synthetic certificate
hashes in memory so install, list, and delete flows are stable across tests
without adding full certificate parsing to simulator state.
Signed firmware and log actions enqueue the expected status-notification
sequence without adding file transfer or cryptographic verification to the core
runtime.
Trace-frame logging runs through parsed-frame redaction before emitting logs,
so known password, ID token, and URL-contained credential values are not
printed in frame traces.
Detailed security support, retention, and firmware limitations are documented in
[`ocpp-support.md`](ocpp-support.md).

## Terminal UI

The terminal app keeps inline rendering, input routing, snapshots, and file
logging in the main app module.
Command history navigation and tab completion are split into focused
submodules under `src/app/` so editing behavior can evolve without making the
rendering path harder to scan.

## Connector State Notes

The simulator uses one protocol-neutral connector state and maps it to the
active protocol's OCPP status values when payloads are built.
Local transitional states remain available to drive simulator behavior
before being mapped to wire values.

| Local state     | OCPP 1.6 status | OCPP 2.x status | Notes                                     |
| --------------- | --------------- | --------------- | ----------------------------------------- |
| `Available`     | `Available`     | `Available`     | Ready to start.                           |
| `Preparing`     | `Preparing`     | `Occupied`      | Startable pre-transaction state.          |
| `Charging`      | `Charging`      | `Occupied`      | Active OCPP 1.6 transaction state.        |
| `Occupied`      | `Charging`      | `Occupied`      | Active OCPP 2.x transaction state.        |
| `SuspendedEVSE` | `SuspendedEVSE` | `Occupied`      | Applied by zero smart-charging limit.     |
| `SuspendedEV`   | `SuspendedEV`   | `Occupied`      | User-settable simulation state.           |
| `Finishing`     | `Finishing`     | `Occupied`      | Stop sent, cable still logically present. |
| `Reserved`      | `Reserved`      | `Reserved`      | Blocks new starts.                        |
| `Unavailable`   | `Unavailable`   | `Unavailable`   | Blocks new starts.                        |
| `Faulted`       | `Faulted`       | `Faulted`       | Blocks new starts.                        |

## Schema Validation

Checked-in JSON schemas under [`schemas/`](../schemas/) are the source of truth
for payload validation tests.
Runtime strict-validation behavior is documented in
[`ocpp-support.md`](ocpp-support.md).

Version-specific builders and tests are kept even when the current payloads are
identical.
That keeps protocol drift local when OCPP 2.1 diverges from OCPP 2.0.1.
