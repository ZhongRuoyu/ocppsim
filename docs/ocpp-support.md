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
- simulated diagnostics and firmware update success flows.

The OCPP 1.6 Security Whitepaper extension is out of scope for now.
Recognized extension actions are logged and return `NotSupported` instead of
being treated as unknown actions.
The simulator does not implement OCPP security profiles, client certificates,
certificate-management actions, or security event workflows.

Incoming response status tokens are recognized from the checked-in 1.6,
2.0.1, and 2.1 schemas.

## OCPP 2.x Support Matrix

OCPP 2.0.1 and OCPP 2.1 support the same common subset today.
For 2.x actions with larger schemas, support is field-level and conservative:
the simulator consumes the identifiers, status fields, variables, local-list
version, firmware location, and simplified smart-charging schedule data needed
for its implemented behavior, while ignoring optional fields that do not
affect that behavior unless strict mode is enabled.

| Action                       | OCPP 2.0.1 | OCPP 2.1  |
| ---------------------------- | ---------- | --------- |
| `Authorize`                  | Supported  | Supported |
| `BootNotification`           | Supported  | Supported |
| `CancelReservation`          | Supported  | Supported |
| `ChangeAvailability`         | Supported  | Supported |
| `ClearCache`                 | Supported  | Supported |
| `ClearChargingProfile`       | Supported  | Supported |
| `DataTransfer`               | Supported  | Supported |
| `FirmwareStatusNotification` | Supported  | Supported |
| `GetCompositeSchedule`       | Supported  | Supported |
| `GetLocalListVersion`        | Supported  | Supported |
| `GetLog`                     | Supported  | Supported |
| `GetVariables`               | Supported  | Supported |
| `Heartbeat`                  | Supported  | Supported |
| `LogStatusNotification`      | Supported  | Supported |
| `MeterValues`                | Supported  | Supported |
| `RequestStartTransaction`    | Supported  | Supported |
| `RequestStopTransaction`     | Supported  | Supported |
| `ReserveNow`                 | Supported  | Supported |
| `Reset`                      | Supported  | Supported |
| `SendLocalList`              | Supported  | Supported |
| `SetChargingProfile`         | Supported  | Supported |
| `SetVariables`               | Supported  | Supported |
| `StatusNotification`         | Supported  | Supported |
| `TransactionEvent`           | Supported  | Supported |
| `TriggerMessage`             | Supported  | Supported |
| `UnlockConnector`            | Supported  | Supported |
| `UpdateFirmware`             | Supported  | Supported |

All other schema actions are explicitly unsupported until implemented.

## Unsupported OCPP 2.0.1 Actions

- `CertificateSigned`
- `ClearDisplayMessage`
- `ClearVariableMonitoring`
- `ClearedChargingLimit`
- `CostUpdated`
- `CustomerInformation`
- `DeleteCertificate`
- `Get15118EVCertificate`
- `GetBaseReport`
- `GetCertificateStatus`
- `GetChargingProfiles`
- `GetDisplayMessages`
- `GetInstalledCertificateIds`
- `GetMonitoringReport`
- `GetReport`
- `GetTransactionStatus`
- `InstallCertificate`
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
- `SecurityEventNotification`
- `SetDisplayMessage`
- `SetMonitoringBase`
- `SetMonitoringLevel`
- `SetNetworkProfile`
- `SetVariableMonitoring`
- `SignCertificate`
- `UnpublishFirmware`

## Unsupported OCPP 2.1 Actions

- `AFRRSignal`
- `AdjustPeriodicEventStream`
- `BatterySwap`
- `CertificateSigned`
- `ChangeTransactionTariff`
- `ClearDERControl`
- `ClearDisplayMessage`
- `ClearTariffs`
- `ClearVariableMonitoring`
- `ClearedChargingLimit`
- `ClosePeriodicEventStream`
- `CostUpdated`
- `CustomerInformation`
- `DeleteCertificate`
- `Get15118EVCertificate`
- `GetBaseReport`
- `GetCertificateChainStatus`
- `GetCertificateStatus`
- `GetChargingProfiles`
- `GetDERControl`
- `GetDisplayMessages`
- `GetInstalledCertificateIds`
- `GetMonitoringReport`
- `GetPeriodicEventStream`
- `GetReport`
- `GetTariffs`
- `GetTransactionStatus`
- `InstallCertificate`
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
- `SecurityEventNotification`
- `SetDERControl`
- `SetDefaultTariff`
- `SetDisplayMessage`
- `SetMonitoringBase`
- `SetMonitoringLevel`
- `SetNetworkProfile`
- `SetVariableMonitoring`
- `SignCertificate`
- `UnpublishFirmware`
- `UpdateDynamicSchedule`
- `UsePriorityCharging`
- `VatNumberValidation`

## Configuration Mapping

OCPP 1.6 configuration keys are stored in one backing map.
For OCPP 2.0.1 and OCPP 2.1, `GetVariables` and `SetVariables` expose that map
as component `ChargingStation` variables.

Supported variable attribute types are `Actual` and `Target`.
`MinSet` and `MaxSet` return `NotSupportedAttributeType`.
Unknown components return `UnknownComponent`, and unknown variables return
`UnknownVariable`.
Read-only variables reject writes.

| Key                          | Writable | Notes                                         |
| ---------------------------- | -------- | --------------------------------------------- |
| `AllowOfflineTxForUnknownId` | Yes      | Stored as configuration only.                 |
| `AuthorizeRemoteTxRequests`  | Yes      | Controls OCPP 1.6 remote-start authorization. |
| `HeartbeatInterval`          | Yes      | Boot/config changes can restart heartbeats.   |
| `MeterValueSampleInterval`   | Yes      | Stored as configuration only.                 |
| `NumberOfConnectors`         | No       | Derived from startup configuration.           |
| `SupportedFeatureProfiles`   | No       | Advertises implemented feature families.      |
| `WebSocketPingInterval`      | Yes      | Stored as configuration only.                 |

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
