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
- reservation and cancellation,
- connector unlock,
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
OCPP 1.6 `AuthorizationKey` values must be 32 to 40 ASCII hexadecimal
characters.
OCPP 2.0.1 `BasicAuthPassword` values must be 16 to 40 OCPP
`passwordString` characters, and OCPP 2.1 `BasicAuthPassword` values must be
16 to 64 UTF-8 `passwordString` characters.
For OCPP 2.1 this is compatibility support through `SecurityCtrlr`.
The simulator does not implement the full 2.1 `NetworkConfiguration`
component, `ActiveNetworkProfile`, or `SetNetworkProfile` behavior.
Security password values are write-only on readback and trigger reconnect when
changed on an active connection.
Certificate-management and signed-firmware actions keep an in-memory synthetic
certificate store and obvious invalid-value detection, but they do not perform
full PKI validation, OCSP/CRL checks, real CSR generation, firmware binary
verification, or file transfer.
Known password, id token, and URL-contained credential values are redacted from
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
  certificate, and OCPP 2.1 request ids are correlated.
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
| `ReserveNow`                 | Supported  | Supported  |
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

| Key                              | Writable | Notes                                                |
| -------------------------------- | -------- | ---------------------------------------------------- |
| `AdditionalRootCertificateCheck` | 2.x only | Read-only for OCPP 1.6 whitepaper behavior.          |
| `AllowOfflineTxForUnknownId`     | Yes      | Stored as configuration only.                        |
| `AllowSecurityProfileDowngrade`  | Yes      | OCPP 1.6 still rejects profile downgrades.           |
| `AuthorizeRemoteTxRequests`      | Yes      | Controls OCPP 1.6 remote-start authorization.        |
| `AuthorizationKey`               | Yes      | Write-only 1.6 Basic Auth password.                  |
| `BasicAuthPassword`              | Yes      | Write-only 2.x Basic Auth password.                  |
| `CertificateSignedMaxChainSize`  | 2.x only | Read-only for OCPP 1.6; maximum 10000 characters.    |
| `CertificateStoreMaxLength`      | No       | Maximum in-memory certificate entries.               |
| `CpoName`                        | Yes      | Stored security organization name.                   |
| `HeartbeatInterval`              | Yes      | Boot/config changes can restart heartbeats.          |
| `MaxCertificateChainSize`        | Yes      | Alias for certificate chain size limit, max 10000.   |
| `MeterValueSampleInterval`       | Yes      | Stored as configuration only.                        |
| `NumberOfConnectors`             | No       | Derived from startup configuration.                  |
| `OrganizationName`               | Yes      | Alias for stored security organization name.         |
| `SecurityProfile`                | Yes      | Controls transport profile validation and reconnect. |
| `SupportedFeatureProfiles`       | No       | Advertises implemented feature families.             |
| `SupportedFileTransferProtocols` | Yes      | Controls file-transfer URI schemes.                  |
| `WebSocketPingInterval`          | Yes      | Stored as configuration only.                        |

`SupportedFileTransferProtocols` is enforced for OCPP 1.6 diagnostics,
OCPP 1.6 `GetLog`, OCPP 1.6 `SignedUpdateFirmware`, OCPP 2.x `GetLog`, and
OCPP 2.x `UpdateFirmware`.
The original OCPP 1.6 `UpdateFirmware` request is rejected for whitepaper
conformance before any URI scheme is considered.

## Behavioral Semantics

The simulator validates required fields before mutating state for supported
inbound flows.
Malformed payloads receive `FormationViolation` CALLERROR responses instead of
falling back to connector zero, empty strings, or silent no-op behavior.

Transaction starts are accepted only on known, startable connectors.
Unavailable, faulted, reserved, occupied, finishing, and already-active
connectors reject starts.
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

Availability changes to `Inoperative` are scheduled when the target connector
has an active transaction.
The scheduled `Unavailable` state is applied after the stop or transaction-end
event is acknowledged.

Reservations are keyed by reservation id and duplicate ids are rejected.
The local authorization list version is stored, but local authorization list
contents are not used to make authorization decisions.

Smart charging is an effective-limit simulation, not a full profile engine.
One profile is stored per connector.
The first supported limit in the stored profile drives connector suspension and
composite schedule output.
A connector with no stored profile returns `Rejected` for composite schedule
requests.
A stored limit of `0` is different from no profile: it is accepted and drives
the connector into a suspended state while the transaction remains active.
`ClearChargingProfile` honors connector or EVSE, profile id, purpose, and stack
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
Schema-invalid requests return `FormationViolation` without mutating simulator
state.

Without strict mode, supported inbound actions still validate the fields needed
by implemented behavior, but optional fields outside that behavior may be
ignored.

## Schema Source

The canonical schema paths used by tests are under `schemas/`:

- `schemas/1.6`
- `schemas/2.0.1`
- `schemas/2.1`
