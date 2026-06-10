# Design

`ocppsim` is organized around a stateful simulator event loop.
The event loop bridges three streams of work:

- user commands from the terminal UI,
- outbound OCPP CALL messages queued by simulator workflows,
- inbound WebSocket frames from the CSMS.

## Runtime Model

The Tokio runtime owns one `Simulator` value for the active session.
The simulator stores connector state, local transactions, meter values,
reservations, scheduled availability changes, charging profiles,
configuration entries, pending CALL context, and an outbound queue.
Security-related simulator state is stored beside the common state: selected
security profile, in-memory synthetic certificates, recent security events,
and certificate/log/firmware limits.
Security events also track notification delivery state so they can be replayed
after reconnect until the CSMS acknowledges them.

When connected, the loop sends the next queued CALL only when there is no
pending request.
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

## Protocol Dispatch

Inbound CALL frames are parsed into OCPP-J frame types, then dispatched by the
configured protocol version.
OCPP 1.6 and OCPP 2.x have separate dispatch modules because action names,
payload shapes, and response statuses differ.
Typed request extractors validate required fields before mutating simulator
state for state-changing flows such as remote start, remote stop, and
availability changes, plus supported connector-addressed flows such as
reservations, local-list updates, unlock requests, trigger messages, firmware
updates, and smart-charging requests.
Malformed supported payloads return `FormationViolation` instead of defaulting
missing identifiers.
OCPP 1.6 standard `TriggerMessage` and security `ExtendedTriggerMessage` use
separate parsers so security-extension triggers are not accepted on the base
action in non-strict mode.

When strict mode is enabled through `--strict` or profile `strict = true`,
inbound CALL payloads are also validated against the checked-in request schemas
before dispatch.
Strict schema failures return `FormationViolation` and do not mutate simulator
state.

The 2.x dispatcher intentionally supports only the common subset that maps to
the simulator's implemented behavior plus overlapping certificate and security
flows.
Known but unsupported 2.x actions return `NotSupported`; unknown action names
return `NotImplemented`.

## State And Workflows

The simulator keeps protocol-neutral state where possible.
Outbound workflow methods translate that state into version-specific OCPP
payloads at enqueue time.
OCPP 1.6 uses `StartTransaction` and `StopTransaction`; OCPP 2.0.1 and OCPP
2.1 use `TransactionEvent`.
Transaction starts are accepted only for known connectors that are currently
startable.
Reserved, unavailable, faulted, occupied, finishing, or already-active
connectors reject new starts.
When a remote start omits a connector or EVSE, the simulator chooses the first
startable connector.

`ChangeAvailability` requests that make an active connector inoperative are
stored as scheduled changes.
The scheduled state is applied after the active transaction stops and is
acknowledged, then a status notification is queued.
Online start and stop workflows queue status notifications only after the
outbound CALL result, CALLERROR, or timeout has finalized local state.
This keeps status payloads from reporting stale intermediate states such as
`Finishing` after the transaction has been restored or completed.
Availability changes and post-transaction cleanup share connector transition
helpers so reservations and scheduled availability are resolved consistently.
Duplicate reservation ids are rejected so one reservation cannot orphan a
previous connector in `Reserved` state.

Configuration is stored as a map of OCPP 1.6-style keys.
OCPP 1.6 exposes that map through `GetConfiguration` and
`ChangeConfiguration`.
OCPP 2.0.1 and OCPP 2.1 expose the same backing values through
`GetVariables` and `SetVariables` for component `ChargingStation` or
`SecurityCtrlr`.
The OCPP 2.1 `NetworkConfiguration` component is intentionally outside the
current simulator model; OCPP 2.1 Basic Auth changes use the legacy-compatible
`SecurityCtrlr` variable path.
Security password values are write-only: they can be changed, but
`GetConfiguration` and `GetVariables` do not return the secret value.

Accepted `BootNotification` responses update the local `HeartbeatInterval`
configuration value and start or restart periodic heartbeats with the interval
returned by the CSMS.

Smart charging stores one effective profile per connector.
The simulator applies the first supported limit value to connector state and
composite schedules.
A missing profile is represented separately from a zero limit: no profile makes
composite schedule requests return `Rejected`, while a zero limit is accepted
and suspends an active connector.
`ClearChargingProfile` honors connector/EVSE, profile id, purpose, and stack
level filters against that simplified store, but it does not model full
profile stacking, recurrency, validity windows, sales tariff data, phase
constraints, or time-window precedence.

Each local connector is modeled as one OCPP 2.x EVSE with `connectorId = 1`.
This keeps 1.6 connector addressing and 2.x EVSE addressing aligned for the
current simulator scope.
Supporting multiple physical connectors per EVSE would require a richer EVSE
model and separate connector state beneath each EVSE.

Security extension behavior is modeled at the simulator boundary.
Transport profile validation checks URL schemes, Basic Auth password format,
and profile 3 certificate/key configuration before connecting.
When CA or client certificate paths are provided, the connection uses a custom
rustls connector with WebPKI roots plus the configured PEM files.
Configured CA and client certificate/key paths are treated as the OCPP-level
certificate prerequisites for security-profile upgrades, because those files
are the real transport trust and identity material used by rustls.
Secure connection setup failures record a local
`InvalidCentralSystemCertificate` security event when the selected profile uses
TLS.
Accepted password changes and higher OCPP 1.6 security profile changes request
a disconnect/reconnect after the CALLRESULT is sent.
Profile-upgrade reconnect failure restores the previous profile and attempts a
fallback reconnect.
Security events remain pending until their `SecurityEventNotification` receives
a CALLRESULT.
Disconnect and security reconnect paths reset queued but unacknowledged events
so they are sent again after the next successful connection.
Certificate-management actions maintain deterministic synthetic certificate
hashes in memory so install, list, and delete flows are stable across tests
without adding full certificate parsing to simulator state.
`AdditionalRootCertificateCheck` is modeled as a Central System root plus one
fallback root; the simulator does not verify the real signing relationship
between those roots.
Signed firmware and log actions enqueue the expected status-notification
sequence, enforce configured URI schemes, and record clear invalid-value events,
but they do not download or upload files, verify firmware binaries, perform
OCSP/CRL checks, or generate real CSRs.
The original OCPP 1.6 `UpdateFirmware` action returns CALLERROR
`NotSupported`; OCPP 1.6 security firmware testing uses
`SignedUpdateFirmware`.
Trace-frame logging runs through parsed-frame redaction before emitting logs,
so known password, id token, and URL-contained credential values are not
printed in frame traces.

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
for tests.
Representative payload tests validate outbound requests for OCPP 1.6, OCPP
2.0.1, and OCPP 2.1.
Supported inbound CALL response payloads are validated against a schema matrix
for all three protocol families.
Additional regression tests cover malformed inbound remote-start and
request-start requests, stricter supported-action payload parsing,
subprotocol negotiation, scheduled availability, duplicate reservations,
transaction-start eligibility, timeout rollback, final status notification
sequencing, filtered charging-profile clearing, certificate install/list/delete,
write-only security configuration, signed-firmware security events, malformed
WebSocket frames, and local mock CSMS WebSocket boot plus
remote-start/meter/remote-stop lifecycles.

Version-specific builders and tests are kept even when the current payloads are
identical.
That keeps protocol drift local when OCPP 2.1 diverges from OCPP 2.0.1.
