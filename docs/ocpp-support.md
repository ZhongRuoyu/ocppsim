# OCPP Support

This page describes the current implementation scope.
It is intentionally conservative: actions are listed as supported only when the
simulator builds or handles protocol-shaped payloads and has focused tests.

## OCPP 1.6

OCPP 1.6 base JSON-schema actions are the complete support target.
Supported behavior includes:

- boot, heartbeat, authorize, status, meter, and data transfer messages,
- local and remote transaction start and stop, including OCPP 1.6
  remote-start pre-authorization when `AuthorizeRemoteTxRequests` is true,
- configuration get and change,
- clear cache,
- availability changes,
- local authorization list version and updates,
- partial reservation support and cancellation,
- idle connector unlock,
- simplified smart-charging profile set, filtered clear, and composite
  schedule,
- trigger message for implemented message types,
- simulated diagnostics success flows for configured URI schemes,
- OCPP 1.6 Security Whitepaper certificate installation, deletion, listing,
  certificate signing, signed firmware status, log status, security event, and
  extended trigger-message flows.

Security support is intentionally simulator-level.
The transport layer supports security profiles 1 and 2 with HTTP Basic
authentication and profile 3 with configured client certificate/key paths.
OCPP 1.6 `AuthorizationKey` values must contain an even number of ASCII
hexadecimal characters, from 32 to 40, representing a 16- to 20-byte key.
HTTP Basic authentication uses the decoded key bytes.
OCPP 2.0.1 `BasicAuthPassword` values must be 16 to 40 OCPP
`passwordString` characters, and OCPP 2.1 `BasicAuthPassword` values must be
16 to 64 UTF-8 `passwordString` characters.
OCPP 2.x passwords are used as UTF-8 text without hexadecimal decoding.
For OCPP 2.1 this is compatibility support through `SecurityCtrlr`.
The simulator does not implement the full 2.1 `NetworkConfiguration`
component, `ActiveNetworkProfile`, or `SetNetworkProfile` behavior.
Security password values are write-only on readback and trigger reconnect when
changed on an active connection.
Certificate-management and signed-firmware actions keep an in-memory synthetic
certificate store and obvious invalid-value detection, but they do not perform
full PKI validation, OCSP/CRL checks, real CSR generation, firmware binary
verification, or file transfer.
Known password, ID token, and URL-contained credential values are redacted from
trace-frame logs in both directions.
The original OCPP 1.6 `UpdateFirmware` request is rejected with CALLERROR
`NotSupported`; whitepaper firmware tests should use `SignedUpdateFirmware`.

## OCPP 1.6 Security Whitepaper Coverage

The simulator implements the OCPP message surface and state transitions needed
for CSMS interoperability testing, while keeping cryptographic material
synthetic.

- A01 password update: implemented.
  `AuthorizationKey` is write-only, validated, stored in memory, records a
  security event, and reconnects when changed while connected.
- A02/A03 certificate update: simulated.
  `ExtendedTriggerMessage(SignChargePointCertificate)` can enqueue
  `SignCertificate`, `CertificateSigned` installs a synthetic Charge Point
  certificate, and OCPP 2.1 request ids are correlated when provided.
  Real key generation, CSR creation, and certificate-chain validation are not
  implemented.
- A05 security profile upgrade: implemented at simulator level.
  OCPP 1.6 rejects equal or lower values, checks password, Central System root,
  and configured mTLS prerequisites for higher profiles, reconnects after
  accepted upgrades, and falls back to the previous profile when reconnect
  fails.
- M05 install CA certificate: partially implemented.
  `InstallCertificate` enforces the 5500-character request limit, rejects full
  stores, and keeps deterministic synthetic hashes for list/delete flows.
  `AdditionalRootCertificateCheck` keeps a Central System root plus one
  fallback root, but does not cryptographically verify that the new root was
  signed by the old root.
- N01 log upload: simulated.
  `GetLog` validates supported URI schemes, returns a synthetic filename, and
  enqueues log status notifications.
  No file is uploaded.
- L01 signed firmware: simulated.
  `SignedUpdateFirmware` validates supported URI schemes, emits status
  notifications, records simulated invalid certificate/signature events, and
  original `UpdateFirmware` returns CALLERROR `NotSupported`.
  No firmware is downloaded or cryptographically verified.

Security events are recorded in local simulator state and queued for OCPP
`SecurityEventNotification` delivery until the CSMS acknowledges them with a
CALLRESULT.
Events recorded while disconnected, or queued immediately before a security
reconnect, are replayed after the next successful connection.
Events may also appear in a later simulated security-log export.
Retention is capped by `security-event-limit`; sent events are discarded before
unsent events when the cap is reached.

Synthetic certificate hashes are stable simulator identifiers for tests.
They are not cryptographic certificate fingerprints.

Incoming response status tokens are recognized from the checked-in 1.6,
2.0.1, and 2.1 schemas.

### Trigger Message Scope

OCPP 1.6 standard `TriggerMessage` accepts only the standard schema values:
`BootNotification`, `DiagnosticsStatusNotification`,
`FirmwareStatusNotification`, `Heartbeat`, `MeterValues`, and
`StatusNotification`.
OCPP 1.6 security `ExtendedTriggerMessage` accepts the whitepaper values:
`BootNotification`, `FirmwareStatusNotification`, `Heartbeat`,
`LogStatusNotification`, `MeterValues`, `SignChargePointCertificate`, and
`StatusNotification`.

OCPP 2.0.1 `TriggerMessage` accepts `BootNotification`,
`FirmwareStatusNotification`, `Heartbeat`, `LogStatusNotification`,
`MeterValues`, `SignChargingStationCertificate`, `SignV2GCertificate`,
`StatusNotification`, and `TransactionEvent`.
`SignCombinedCertificate` and `PublishFirmwareStatusNotification` are
schema-valid but return `NotImplemented`.

OCPP 2.1 adds `SignV2G20Certificate` to the accepted trigger list and parses
`CustomTrigger`, which currently returns `NotImplemented`.

## OCPP 2.x Support Matrix

OCPP 2.0.1 and OCPP 2.1 support the same common subset today.
For 2.x actions with larger schemas, support is field-level and conservative:
the simulator consumes the identifiers, status fields, variables, local-list
version, firmware location, and simplified smart-charging schedule data needed
for its implemented behavior, while ignoring optional fields that do not
affect that behavior unless strict mode is enabled.

| Action                       | OCPP 2.0.1 | OCPP 2.1   |
| ---------------------------- | ---------- | ---------- |
| `Authorize`                  | Supported  | Supported  |
| `BootNotification`           | Supported  | Supported  |
| `CancelReservation`          | Supported  | Supported  |
| `CertificateSigned`          | Supported  | Supported  |
| `ChangeAvailability`         | Supported  | Supported  |
| `ClearCache`                 | Supported  | Supported  |
| `ClearChargingProfile`       | Supported  | Supported  |
| `DataTransfer`               | Supported  | Supported  |
| `DeleteCertificate`          | Supported  | Supported  |
| `FirmwareStatusNotification` | Supported  | Supported  |
| `GetCompositeSchedule`       | Supported  | Supported  |
| `GetInstalledCertificateIds` | Supported  | Supported  |
| `GetLocalListVersion`        | Supported  | Supported  |
| `GetLog`                     | Supported  | Supported  |
| `GetVariables`               | Supported  | Supported  |
| `Heartbeat`                  | Supported  | Supported  |
| `InstallCertificate`         | Supported  | Supported  |
| `LogStatusNotification`      | Supported  | Supported  |
| `MeterValues`                | Supported  | Supported  |
| `RequestStartTransaction`    | Supported  | Supported  |
| `RequestStopTransaction`     | Supported  | Supported  |
| `ReserveNow`                 | Partial    | Partial    |
| `Reset`                      | Supported  | Supported  |
| `SecurityEventNotification`  | Supported  | Supported  |
| `SendLocalList`              | Supported  | Supported  |
| `SetChargingProfile`         | Supported  | Supported  |
| `SetVariables`               | Supported  | Supported  |
| `SignCertificate`            | Supported  | Supported  |
| `StatusNotification`         | Supported  | Supported  |
| `TransactionEvent`           | Supported  | Supported  |
| `TriggerMessage`             | Supported* | Supported* |
| `UnlockConnector`            | Supported  | Supported  |
| `UpdateFirmware`             | Supported  | Supported  |

All other schema actions are explicitly unsupported until implemented.
The `TriggerMessage` action is field-level support; see the trigger scope above
for version-specific values.
`ReserveNow` supports reserving and canceling a connector, with the ownership
and expiry limitations described below.

## Unsupported OCPP 2.0.1 Actions

- `ClearDisplayMessage`
- `ClearVariableMonitoring`
- `ClearedChargingLimit`
- `CostUpdated`
- `CustomerInformation`
- `Get15118EVCertificate`
- `GetBaseReport`
- `GetCertificateStatus`
- `GetChargingProfiles`
- `GetDisplayMessages`
- `GetMonitoringReport`
- `GetReport`
- `GetTransactionStatus`
- `NotifyChargingLimit`
- `NotifyCustomerInformation`
- `NotifyDisplayMessages`
- `NotifyEVChargingNeeds`
- `NotifyEVChargingSchedule`
- `NotifyEvent`
- `NotifyMonitoringReport`
- `NotifyReport`
- `PublishFirmware`
- `PublishFirmwareStatusNotification`
- `ReportChargingProfiles`
- `ReservationStatusUpdate`
- `SetDisplayMessage`
- `SetMonitoringBase`
- `SetMonitoringLevel`
- `SetNetworkProfile`
- `SetVariableMonitoring`
- `UnpublishFirmware`

## Unsupported OCPP 2.1 Actions

- `AFRRSignal`
- `AdjustPeriodicEventStream`
- `BatterySwap`
- `ChangeTransactionTariff`
- `ClearDERControl`
- `ClearDisplayMessage`
- `ClearTariffs`
- `ClearVariableMonitoring`
- `ClearedChargingLimit`
- `ClosePeriodicEventStream`
- `CostUpdated`
- `CustomerInformation`
- `Get15118EVCertificate`
- `GetBaseReport`
- `GetCertificateChainStatus`
- `GetCertificateStatus`
- `GetChargingProfiles`
- `GetDERControl`
- `GetDisplayMessages`
- `GetMonitoringReport`
- `GetPeriodicEventStream`
- `GetReport`
- `GetTariffs`
- `GetTransactionStatus`
- `NotifyAllowedEnergyTransfer`
- `NotifyChargingLimit`
- `NotifyCustomerInformation`
- `NotifyDERAlarm`
- `NotifyDERStartStop`
- `NotifyDisplayMessages`
- `NotifyEVChargingNeeds`
- `NotifyEVChargingSchedule`
- `NotifyEvent`
- `NotifyMonitoringReport`
- `NotifyPeriodicEventStream`
- `NotifyPriorityCharging`
- `NotifyReport`
- `NotifySettlement`
- `NotifyWebPaymentStarted`
- `OpenPeriodicEventStream`
- `PublishFirmware`
- `PublishFirmwareStatusNotification`
- `PullDynamicScheduleUpdate`
- `ReportChargingProfiles`
- `ReportDERControl`
- `RequestBatterySwap`
- `ReservationStatusUpdate`
- `SetDERControl`
- `SetDefaultTariff`
- `SetDisplayMessage`
- `SetMonitoringBase`
- `SetMonitoringLevel`
- `SetNetworkProfile`
- `SetVariableMonitoring`
- `UnpublishFirmware`
- `UpdateDynamicSchedule`
- `UsePriorityCharging`
- `VatNumberValidation`

## Configuration Mapping

OCPP 1.6 configuration keys are stored in one backing map.
For OCPP 2.0.1 and OCPP 2.1, `GetVariables` and `SetVariables` expose that map
as component `ChargingStation` or `SecurityCtrlr` variables.
OCPP 2.1 `NetworkConfiguration` variables are not implemented; Basic Auth
password changes use the backwards-compatible `SecurityCtrlr` mapping.

Supported variable attribute types are `Actual` and `Target`.
`MinSet` and `MaxSet` return `NotSupportedAttributeType`.
Unknown components return `UnknownComponent`, and unknown variables return
`UnknownVariable`.
Read-only variables reject writes.

The local `config list`, `config get`, and `config set` commands use the same
configuration map and write validation.
Boolean values accept `true` or `false`, ignoring case.
Intervals and retry counts accept integers from 0 to 2147483647, except
`HeartbeatInterval`, which must be positive.
Organization names must be nonempty, and file-transfer protocol lists accept
`HTTP`, `HTTPS`, `FTP`, and `FTPS`.
Security passwords and profile changes retain their protocol-specific checks.
Values must fit the active protocol's configuration-value length limit and
cannot contain control characters.
Invalid writes leave values and origins unchanged.
Local writes record origin `local` and do not emit configuration-change events.
Variable monitoring remains unsupported.

| Key                              | Writable | Notes                                              |
| -------------------------------- | -------- | -------------------------------------------------- |
| `AdditionalRootCertificateCheck` | No       | Read-only SecurityCtrlr capability flag.           |
| `AllowOfflineTxForUnknownId`     | Yes      | Stored as configuration only.                      |
| `AllowSecurityProfileDowngrade`  | Yes      | OCPP 1.6 still rejects profile downgrades.         |
| `AuthorizeRemoteTxRequests`      | Yes      | Controls OCPP 1.6 remote-start authorization.      |
| `AuthorizationKey`               | Yes      | Write-only 1.6 Basic Auth password.                |
| `BasicAuthPassword`              | Yes      | Write-only 2.x Basic Auth password.                |
| `CertificateSignedMaxChainSize`  | No       | Read-only compatibility alias.                     |
| `CertificateStoreMaxLength`      | No       | Maximum in-memory certificate entries.             |
| `CertSigningRepeatTimes`         | Yes      | Stored as configuration only.                      |
| `CertSigningWaitMinimum`         | Yes      | Stored as configuration only.                      |
| `CpoName`                        | Yes      | Stored security organization name.                 |
| `HeartbeatInterval`              | Yes      | Boot/config changes can restart heartbeats.        |
| `MaxCertificateChainSize`        | No       | Read-only certificate chain size limit, max 10000. |
| `MeterValueSampleInterval`       | Yes      | Stored as configuration only.                      |
| `NumberOfConnectors`             | No       | Derived from startup configuration.                |
| `OrganizationName`               | Yes      | Alias for stored security organization name.       |
| `SecurityProfile`                | 1.6 only | 2.x reports active profile as read-only.           |
| `SupportedFeatureProfiles`       | No       | Advertises implemented feature families.           |
| `SupportedFileTransferProtocols` | Yes      | Controls file-transfer URI schemes.                |
| `WebSocketPingInterval`          | Yes      | Stored as configuration only.                      |

`SupportedFileTransferProtocols` is enforced for OCPP 1.6 diagnostics,
OCPP 1.6 `GetLog`, OCPP 1.6 `SignedUpdateFirmware`, OCPP 2.x `GetLog`, and
OCPP 2.x `UpdateFirmware`.
The original OCPP 1.6 `UpdateFirmware` request is rejected for whitepaper
conformance before any URI scheme is considered.

## Meter Readings

The `measurand` command reads and sets any measurand defined by the active
OCPP 1.6, 2.0.1, or 2.1 schema.
Names are case insensitive in commands and completion; outbound messages
always use canonical OCPP names.
Unknown or unsupported names are rejected.
Lists include only set readings supported by the active protocol.
Named queries show `unset` when no reading has been supplied.

The command uses the following canonical units.
OCPP permits scaled alternatives for some quantities, but the command has no
unit argument, so input always uses the units below.

| Quantity                                                | Input unit                   |
| ------------------------------------------------------- | ---------------------------- |
| Active energy, energy requests, battery capacity        | Wh                           |
| Reactive energy                                         | varh                         |
| Apparent energy                                         | VAh                          |
| Active power, offered power, power limits and setpoints | W                            |
| Reactive power                                          | var                          |
| Current and current limits                              | A                            |
| Voltage and voltage limits                              | V                            |
| Frequency                                               | Hz                           |
| Temperature                                             | Celsius                      |
| Fan speed                                               | RPM                          |
| SoC and display SoC values                              | Percent, from 0 to 100       |
| Display remaining times                                 | s                            |
| Power factor                                            | Unitless ratio, from -1 to 1 |
| Display charging-complete and inlet-hot flags           | Unitless 0 or 1              |

Values must be finite numbers.
`Energy.Active.Import.Register` retains its signed integer Wh representation
for compatibility with `meter` and OCPP 1.6 transaction boundaries.
Other numeric readings accept decimals.
Plain signed numbers set absolute values, allowing negative temperatures,
net values, energy requests, and setpoints.
The prefixes `+=` and `-=` adjust an existing reading in its canonical unit.
The operator and operand form one token, such as `+=100` or `-=2.5`.
This applies to `measurand`, `meter`, `soc`, and their send shorthands.

Adjustments require a set, unexpired reading and validate the result using
the same rules as absolute assignments.
Invalid results and overflow leave the reading unchanged.
`Display.ChargingComplete` and `Display.InletHot` are absolute-only flags,
so all arithmetic on them is rejected, including zero adjustments.
Integer arithmetic is exact across signed and unsigned 64-bit values.
Integer literals and results outside the supported range are rejected
instead of being rounded to floating point.
Use integer operands for large integer readings; lossy mixed conversions
and nonzero adjustments too small to change the stored value are rejected.

OCPP 1.6 encodes values as strings and OCPP 2.x uses JSON numbers.
Integer readings retain their precision.
OCPP 1.6 omits the unit field for frequency, RPM, and power factor because
its unit enumeration cannot represent Hz, RPM, or a unitless ratio.
OCPP 2.x also omits the unit for unitless readings.
EV parameters use location `EV`, station temperature and fan speed use
`Body`, and frequency uses `Inlet`.
Other readings use the default outlet location without a phase qualifier.

Readings are shared between the generic command and the `meter`/`soc`
aliases, independently for each connector.
The Wh register starts at zero; other readings are unset until supplied.
The `measurand` setter updates local state only.
`send-measurand <connector> <measurand> [<value>]` sends one named reading,
optionally setting or adjusting it first.
It shares validation, case-insensitive names, and protocol-aware completion
with `measurand`.
Invalid updates cancel sending without changing stored readings.
Valid updates remain local when disconnected or awaiting boot acceptance.
The send uses `MeterValues`, or `TransactionEvent` during an active
OCPP 2.x transaction.
Triggered `MeterValues` and OCPP 2.x transaction-boundary or triggered
`TransactionEvent` messages include the set readings for the active protocol.
`send-meter` and `send-soc` send only their respective measurands.

The simulator assigns reading lifetimes by meaning:

- Connector lifetime: cumulative active, reactive, and apparent energy
  registers, including net and local-generation registers; frequency;
  station temperature; and fan speed.
  These persist until changed or the connector is removed.
- Transaction lifetime: current, power, power factor, voltage, offered values
  and limits, energy intervals, cable-loss energy, SoC, display parameters,
  and EV energy requests.
  Values set while idle prepare the next transaction.
  They expire after an acknowledged stop, a finalized offline stop, or
  cancellation of a created transaction.
- OCPP 2.1 cable-loss energy resets to zero at transaction start when it has
  been configured.
  Unset readings remain unset.
  The local-generation energy register remains cumulative; OCPP permits
  retaining it across transactions.

Queued terminal messages retain their original snapshots.
Suspension, disconnection, and failed stops preserve the continuing
transaction's readings.
Rejected starts that never create a transaction preserve prepared values.
Cleanup checks the local transaction ID, so old responses cannot clear
new readings.
These are manually controlled samples: interval and instantaneous values are
not recalculated automatically within a transaction.

## Behavioral Semantics

The simulator validates required fields before mutating state for supported
inbound flows.
Malformed CALL payloads for supported actions receive format-violation
CALLERROR responses instead of falling back to connector zero, empty strings,
or silent no-op behavior.
The wire error code is `FormationViolation` for OCPP 1.6 and
`FormatViolation` for OCPP 2.x.
Malformed OCPP frames that cannot be parsed receive `ProtocolError`.
Inbound CSMS `CALL` and OCPP 2.1 `SEND` message ids are tracked for the active
WebSocket connection.
Duplicate `CALL` ids receive `OccurrenceConstraintViolation` before dispatch,
and duplicate `SEND` ids are logged and dropped without mutating state.
Each connection retains at most 100,000 distinct inbound message ids.
The next distinct id drops the connection before dispatch; reconnect to start a
new connection with empty tracking state.
Earlier ids are retained until then so duplicate detection remains reliable.

For OCPP 1.6 and OCPP 2.x, station-initiated OCPP messages are held until
`BootNotification` is accepted.
Reports explicitly requested through `TriggerMessage` or
`ExtendedTriggerMessage` can be sent during `Pending` registration.
Those reports may pass queued station-initiated messages, which remain held.
For OCPP 1.6, `Pending` and `Rejected` registration states block
station-initiated requests until a later accepted boot response.
Reconnects send `BootNotification` only when registration is not accepted yet or
the current boot payload differs from the last queued boot payload, and only
after any non-accepted boot retry interval has expired.
All supported versions honor the CSMS interval for `Pending` and `Rejected`
responses, with a short fallback delay when the interval is zero.
Changing the connection URL, charge point identity, protocol, or security
transport settings clears cached registration state so the next connection
sends `BootNotification` even when the boot payload is otherwise unchanged.
Manual `boot` commands respect the retry interval.
Trigger-message boot requests bypass the interval as allowed by OCPP.
For OCPP 1.6, inbound CSMS calls are ignored without an OCPP response while
boot registration is `Rejected`.
`RemoteStartTransaction` and `RemoteStopTransaction` receive a `Rejected`
response while registration is `Pending` or awaiting a boot response.
For OCPP 2.x, `RequestStartTransaction` and `RequestStopTransaction` receive a
`Rejected` response while registration is `Pending`, `Rejected`, or awaiting a
boot response.

Transaction starts are accepted only on known, startable connectors.
Unavailable, faulted, reserved, occupied, and finishing connectors reject
starts.
Already-active connectors reject local starts and OCPP 1.6 remote starts.
For OCPP 2.x `RequestStartTransaction`, an active requested EVSE is accepted
with the existing `transactionId` only while the local transaction has not yet
been authorized by `TransactionEventResponse`.
Already-authorized active EVSEs are rejected.
When a remote start omits a connector or EVSE, the first startable connector is
chosen.

For OCPP 1.6, `AuthorizeRemoteTxRequests` defaults to `true`.
In that mode, an accepted `RemoteStartTransaction` queues `Authorize` first.
The simulator only sends `StartTransaction` after the authorization response is
`Accepted`.
Other authorization statuses, including `ConcurrentTx`, are logged and stop the
remote-start attempt.
When `AuthorizeRemoteTxRequests` is set to `false`, the simulator starts the
transaction immediately and then applies the eventual `StartTransaction.conf`
status from the CSMS.
A non-accepted authorization status retains the CSMS transaction ID and ends
that transaction with `StopTransaction` reason `DeAuthorized`.
If a local stop is already queued, its timestamp and meter reading are retained
and its reason is updated instead of sending a second stop.
The local transaction is cleared after the stop is acknowledged.
If the outbound queue is full or the stop fails, the transaction remains
suspended with its CSMS ID, and a later local stop retries `DeAuthorized`.

For OCPP 2.x, a non-accepted `TransactionEventResponse.idTokenInfo.status`
for a `Started` or `Updated` event deauthorizes the transaction before queueing
an `Ended` event.
A queued end is reused with its original sequence number, timestamp, and meter
reading, and its reason is changed to `DeAuthorized`.
Queue pressure in this flow does not disconnect the simulator.
If the end cannot be queued, fails, times out, or receives an invalid response
in strict mode, the transaction remains suspended.
A later local or remote stop retains the `DeAuthorized` reason and
`Deauthorized` trigger.
Charging profile changes and later authorization responses cannot restore
charging, and remote starts cannot reuse the transaction before it ends.

Availability changes to `Inoperative` are scheduled when the target connector
has an active transaction.
The scheduled `Unavailable` state is applied after the stop or transaction-end
event is acknowledged.

Reservations are keyed by reservation ID and duplicate IDs are rejected.
Reservation support is partial in every version: token ownership and expiry
fields are checked for presence but are not retained or enforced.
Reservations do not expire automatically, and even the reservation owner cannot
start a transaction while the connector remains reserved.
Cancel the reservation before starting a transaction.
The local authorization list version is stored, but local authorization list
contents are not used to make authorization decisions.

OCPP 1.6 `UnlockConnector` supports idle connectors only.
It returns `UnlockFailed` when a transaction is active and does not implement
the transaction-stop sequence required by the complete unlock workflow.
Stop the transaction and wait for its acknowledgement before requesting unlock.

Smart charging is an effective-limit simulation, not a full profile engine.
One profile is stored per connector.
The first supported limit in the stored profile drives connector suspension and
composite schedule output.
Charging profiles supplied with accepted `RemoteStartTransaction` and
`RequestStartTransaction` requests are applied through the same simplified
store.
A connector with no stored profile returns `Rejected` for composite schedule
requests.
Composite schedules retain the effective profile's `A` or `W` unit.
Requests for a different unit return `Rejected`; voltage and phase assumptions
for conversion are not modeled.
Minimal non-strict profiles that omit the unit retain the simulator's `W`
default.
OCPP 1.6 `GetCompositeSchedule` rejects invalid `chargingRateUnit` values with
CALLERROR `PropertyConstraintViolation`.
A stored limit of `0` is different from no profile: it is accepted and drives
the connector into a suspended state while the transaction remains active.
Stored limits apply when a later transaction starts and when a failed stop
restores the active state.
`ClearChargingProfile` honors connector or EVSE, profile ID, purpose, and stack
level filters against that simplified store.
It does not model profile stacking, recurrency, validity windows, phase
constraints, sales tariff data, or time-based schedule precedence.

For OCPP 2.0.1 and OCPP 2.1, each local connector is represented as one EVSE
with `connectorId = 1`.
Multiple physical connectors under one EVSE are not modeled.
OCPP 2.x `UnlockConnector` requests for other `connectorId` values return
`UnknownConnector`.

`Reset`, diagnostics, firmware, and log retrieval requests are simulated
success flows.
They acknowledge the request and enqueue the expected status notifications, but
they do not reboot the process, upload files, or download firmware.

## Strict Inbound Validation

Strict mode can be enabled with `--strict`, global `strict = true`, or
per-charge-point `strict = true`.
When enabled, inbound CALL payloads are validated against the checked-in
request schema before simulator dispatch.
Schema-invalid requests return `FormationViolation` on OCPP 1.6 or
`FormatViolation` on OCPP 2.x without mutating simulator state.
Matching inbound CALLRESULT payloads are validated against the checked-in
response schema before response side effects are applied.
Schema-invalid responses are not applied; OCPP 2.1 also sends
`CALLRESULTERROR` with `FormatViolation`.

Without strict mode, supported inbound actions still validate the fields needed
by implemented behavior, but optional fields outside that behavior may be
ignored.

## Schema Source

The canonical schema paths used by tests are under `schemas/`:

- `schemas/1.6`
- `schemas/2.0.1`
- `schemas/2.1`
