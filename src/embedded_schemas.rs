use crate::ocpp::OcppVersion;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmbeddedSchemaType {
  Request,
  Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmbeddedSchema {
  pub(crate) action: &'static str,
  pub(crate) file_name: &'static str,
  pub(crate) relative_path: &'static str,
  pub(crate) text: &'static str,
}

macro_rules! embedded_schema {
  ($version:literal, $file_name:expr, $action:literal) => {
    EmbeddedSchema {
      action: $action,
      file_name: $file_name,
      relative_path: concat!("schemas/", $version, "/", $file_name),
      text: include_str!(concat!("../schemas/", $version, "/", $file_name)),
    }
  };
}

macro_rules! v1_6_request_schema {
  ($action:literal) => {
    embedded_schema!("1.6", concat!($action, ".json"), $action)
  };
}

macro_rules! v1_6_response_schema {
  ($action:literal) => {
    embedded_schema!("1.6", concat!($action, "Response.json"), $action)
  };
}

macro_rules! v2_x_request_schema {
  ($version:literal, $action:literal) => {
    embedded_schema!($version, concat!($action, "Request.json"), $action)
  };
  ($version:literal, $action:literal, $file_name:literal) => {
    embedded_schema!($version, $file_name, $action)
  };
}

macro_rules! v2_x_response_schema {
  ($version:literal, $action:literal) => {
    embedded_schema!($version, concat!($action, "Response.json"), $action)
  };
}

const V1_6_REQUEST_SCHEMAS: &[EmbeddedSchema] = &[
  v1_6_request_schema!("Authorize"),
  v1_6_request_schema!("BootNotification"),
  v1_6_request_schema!("CancelReservation"),
  v1_6_request_schema!("ChangeAvailability"),
  v1_6_request_schema!("ChangeConfiguration"),
  v1_6_request_schema!("ClearCache"),
  v1_6_request_schema!("ClearChargingProfile"),
  v1_6_request_schema!("DataTransfer"),
  v1_6_request_schema!("DiagnosticsStatusNotification"),
  v1_6_request_schema!("FirmwareStatusNotification"),
  v1_6_request_schema!("GetCompositeSchedule"),
  v1_6_request_schema!("GetConfiguration"),
  v1_6_request_schema!("GetDiagnostics"),
  v1_6_request_schema!("GetLocalListVersion"),
  v1_6_request_schema!("Heartbeat"),
  v1_6_request_schema!("MeterValues"),
  v1_6_request_schema!("RemoteStartTransaction"),
  v1_6_request_schema!("RemoteStopTransaction"),
  v1_6_request_schema!("ReserveNow"),
  v1_6_request_schema!("Reset"),
  v1_6_request_schema!("SendLocalList"),
  v1_6_request_schema!("SetChargingProfile"),
  v1_6_request_schema!("StartTransaction"),
  v1_6_request_schema!("StatusNotification"),
  v1_6_request_schema!("StopTransaction"),
  v1_6_request_schema!("TriggerMessage"),
  v1_6_request_schema!("UnlockConnector"),
  v1_6_request_schema!("UpdateFirmware"),
];

const V1_6_RESPONSE_SCHEMAS: &[EmbeddedSchema] = &[
  v1_6_response_schema!("Authorize"),
  v1_6_response_schema!("BootNotification"),
  v1_6_response_schema!("CancelReservation"),
  v1_6_response_schema!("ChangeAvailability"),
  v1_6_response_schema!("ChangeConfiguration"),
  v1_6_response_schema!("ClearCache"),
  v1_6_response_schema!("ClearChargingProfile"),
  v1_6_response_schema!("DataTransfer"),
  v1_6_response_schema!("DiagnosticsStatusNotification"),
  v1_6_response_schema!("FirmwareStatusNotification"),
  v1_6_response_schema!("GetCompositeSchedule"),
  v1_6_response_schema!("GetConfiguration"),
  v1_6_response_schema!("GetDiagnostics"),
  v1_6_response_schema!("GetLocalListVersion"),
  v1_6_response_schema!("Heartbeat"),
  v1_6_response_schema!("MeterValues"),
  v1_6_response_schema!("RemoteStartTransaction"),
  v1_6_response_schema!("RemoteStopTransaction"),
  v1_6_response_schema!("ReserveNow"),
  v1_6_response_schema!("Reset"),
  v1_6_response_schema!("SendLocalList"),
  v1_6_response_schema!("SetChargingProfile"),
  v1_6_response_schema!("StartTransaction"),
  v1_6_response_schema!("StatusNotification"),
  v1_6_response_schema!("StopTransaction"),
  v1_6_response_schema!("TriggerMessage"),
  v1_6_response_schema!("UnlockConnector"),
  v1_6_response_schema!("UpdateFirmware"),
];

const V2_0_1_REQUEST_SCHEMAS: &[EmbeddedSchema] = &[
  v2_x_request_schema!("2.0.1", "Authorize"),
  v2_x_request_schema!("2.0.1", "BootNotification"),
  v2_x_request_schema!("2.0.1", "CancelReservation"),
  v2_x_request_schema!("2.0.1", "CertificateSigned"),
  v2_x_request_schema!("2.0.1", "ChangeAvailability"),
  v2_x_request_schema!("2.0.1", "ClearCache"),
  v2_x_request_schema!("2.0.1", "ClearChargingProfile"),
  v2_x_request_schema!("2.0.1", "ClearDisplayMessage"),
  v2_x_request_schema!("2.0.1", "ClearVariableMonitoring"),
  v2_x_request_schema!("2.0.1", "ClearedChargingLimit"),
  v2_x_request_schema!("2.0.1", "CostUpdated"),
  v2_x_request_schema!("2.0.1", "CustomerInformation"),
  v2_x_request_schema!("2.0.1", "DataTransfer"),
  v2_x_request_schema!("2.0.1", "DeleteCertificate"),
  v2_x_request_schema!("2.0.1", "FirmwareStatusNotification"),
  v2_x_request_schema!("2.0.1", "Get15118EVCertificate"),
  v2_x_request_schema!("2.0.1", "GetBaseReport"),
  v2_x_request_schema!("2.0.1", "GetCertificateStatus"),
  v2_x_request_schema!("2.0.1", "GetChargingProfiles"),
  v2_x_request_schema!("2.0.1", "GetCompositeSchedule"),
  v2_x_request_schema!("2.0.1", "GetDisplayMessages"),
  v2_x_request_schema!("2.0.1", "GetInstalledCertificateIds"),
  v2_x_request_schema!("2.0.1", "GetLocalListVersion"),
  v2_x_request_schema!("2.0.1", "GetLog"),
  v2_x_request_schema!("2.0.1", "GetMonitoringReport"),
  v2_x_request_schema!("2.0.1", "GetReport"),
  v2_x_request_schema!("2.0.1", "GetTransactionStatus"),
  v2_x_request_schema!("2.0.1", "GetVariables"),
  v2_x_request_schema!("2.0.1", "Heartbeat"),
  v2_x_request_schema!("2.0.1", "InstallCertificate"),
  v2_x_request_schema!("2.0.1", "LogStatusNotification"),
  v2_x_request_schema!("2.0.1", "MeterValues"),
  v2_x_request_schema!("2.0.1", "NotifyChargingLimit"),
  v2_x_request_schema!("2.0.1", "NotifyCustomerInformation"),
  v2_x_request_schema!("2.0.1", "NotifyDisplayMessages"),
  v2_x_request_schema!("2.0.1", "NotifyEVChargingNeeds"),
  v2_x_request_schema!("2.0.1", "NotifyEVChargingSchedule"),
  v2_x_request_schema!("2.0.1", "NotifyEvent"),
  v2_x_request_schema!("2.0.1", "NotifyMonitoringReport"),
  v2_x_request_schema!("2.0.1", "NotifyReport"),
  v2_x_request_schema!("2.0.1", "PublishFirmware"),
  v2_x_request_schema!("2.0.1", "PublishFirmwareStatusNotification"),
  v2_x_request_schema!("2.0.1", "ReportChargingProfiles"),
  v2_x_request_schema!("2.0.1", "RequestStartTransaction"),
  v2_x_request_schema!("2.0.1", "RequestStopTransaction"),
  v2_x_request_schema!("2.0.1", "ReservationStatusUpdate"),
  v2_x_request_schema!("2.0.1", "ReserveNow"),
  v2_x_request_schema!("2.0.1", "Reset"),
  v2_x_request_schema!("2.0.1", "SecurityEventNotification"),
  v2_x_request_schema!("2.0.1", "SendLocalList"),
  v2_x_request_schema!("2.0.1", "SetChargingProfile"),
  v2_x_request_schema!("2.0.1", "SetDisplayMessage"),
  v2_x_request_schema!("2.0.1", "SetMonitoringBase"),
  v2_x_request_schema!("2.0.1", "SetMonitoringLevel"),
  v2_x_request_schema!("2.0.1", "SetNetworkProfile"),
  v2_x_request_schema!("2.0.1", "SetVariableMonitoring"),
  v2_x_request_schema!("2.0.1", "SetVariables"),
  v2_x_request_schema!("2.0.1", "SignCertificate"),
  v2_x_request_schema!("2.0.1", "StatusNotification"),
  v2_x_request_schema!("2.0.1", "TransactionEvent"),
  v2_x_request_schema!("2.0.1", "TriggerMessage"),
  v2_x_request_schema!("2.0.1", "UnlockConnector"),
  v2_x_request_schema!("2.0.1", "UnpublishFirmware"),
  v2_x_request_schema!("2.0.1", "UpdateFirmware"),
];

const V2_0_1_RESPONSE_SCHEMAS: &[EmbeddedSchema] = &[
  v2_x_response_schema!("2.0.1", "Authorize"),
  v2_x_response_schema!("2.0.1", "BootNotification"),
  v2_x_response_schema!("2.0.1", "CancelReservation"),
  v2_x_response_schema!("2.0.1", "CertificateSigned"),
  v2_x_response_schema!("2.0.1", "ChangeAvailability"),
  v2_x_response_schema!("2.0.1", "ClearCache"),
  v2_x_response_schema!("2.0.1", "ClearChargingProfile"),
  v2_x_response_schema!("2.0.1", "ClearDisplayMessage"),
  v2_x_response_schema!("2.0.1", "ClearVariableMonitoring"),
  v2_x_response_schema!("2.0.1", "ClearedChargingLimit"),
  v2_x_response_schema!("2.0.1", "CostUpdated"),
  v2_x_response_schema!("2.0.1", "CustomerInformation"),
  v2_x_response_schema!("2.0.1", "DataTransfer"),
  v2_x_response_schema!("2.0.1", "DeleteCertificate"),
  v2_x_response_schema!("2.0.1", "FirmwareStatusNotification"),
  v2_x_response_schema!("2.0.1", "Get15118EVCertificate"),
  v2_x_response_schema!("2.0.1", "GetBaseReport"),
  v2_x_response_schema!("2.0.1", "GetCertificateStatus"),
  v2_x_response_schema!("2.0.1", "GetChargingProfiles"),
  v2_x_response_schema!("2.0.1", "GetCompositeSchedule"),
  v2_x_response_schema!("2.0.1", "GetDisplayMessages"),
  v2_x_response_schema!("2.0.1", "GetInstalledCertificateIds"),
  v2_x_response_schema!("2.0.1", "GetLocalListVersion"),
  v2_x_response_schema!("2.0.1", "GetLog"),
  v2_x_response_schema!("2.0.1", "GetMonitoringReport"),
  v2_x_response_schema!("2.0.1", "GetReport"),
  v2_x_response_schema!("2.0.1", "GetTransactionStatus"),
  v2_x_response_schema!("2.0.1", "GetVariables"),
  v2_x_response_schema!("2.0.1", "Heartbeat"),
  v2_x_response_schema!("2.0.1", "InstallCertificate"),
  v2_x_response_schema!("2.0.1", "LogStatusNotification"),
  v2_x_response_schema!("2.0.1", "MeterValues"),
  v2_x_response_schema!("2.0.1", "NotifyChargingLimit"),
  v2_x_response_schema!("2.0.1", "NotifyCustomerInformation"),
  v2_x_response_schema!("2.0.1", "NotifyDisplayMessages"),
  v2_x_response_schema!("2.0.1", "NotifyEVChargingNeeds"),
  v2_x_response_schema!("2.0.1", "NotifyEVChargingSchedule"),
  v2_x_response_schema!("2.0.1", "NotifyEvent"),
  v2_x_response_schema!("2.0.1", "NotifyMonitoringReport"),
  v2_x_response_schema!("2.0.1", "NotifyReport"),
  v2_x_response_schema!("2.0.1", "PublishFirmware"),
  v2_x_response_schema!("2.0.1", "PublishFirmwareStatusNotification"),
  v2_x_response_schema!("2.0.1", "ReportChargingProfiles"),
  v2_x_response_schema!("2.0.1", "RequestStartTransaction"),
  v2_x_response_schema!("2.0.1", "RequestStopTransaction"),
  v2_x_response_schema!("2.0.1", "ReservationStatusUpdate"),
  v2_x_response_schema!("2.0.1", "ReserveNow"),
  v2_x_response_schema!("2.0.1", "Reset"),
  v2_x_response_schema!("2.0.1", "SecurityEventNotification"),
  v2_x_response_schema!("2.0.1", "SendLocalList"),
  v2_x_response_schema!("2.0.1", "SetChargingProfile"),
  v2_x_response_schema!("2.0.1", "SetDisplayMessage"),
  v2_x_response_schema!("2.0.1", "SetMonitoringBase"),
  v2_x_response_schema!("2.0.1", "SetMonitoringLevel"),
  v2_x_response_schema!("2.0.1", "SetNetworkProfile"),
  v2_x_response_schema!("2.0.1", "SetVariableMonitoring"),
  v2_x_response_schema!("2.0.1", "SetVariables"),
  v2_x_response_schema!("2.0.1", "SignCertificate"),
  v2_x_response_schema!("2.0.1", "StatusNotification"),
  v2_x_response_schema!("2.0.1", "TransactionEvent"),
  v2_x_response_schema!("2.0.1", "TriggerMessage"),
  v2_x_response_schema!("2.0.1", "UnlockConnector"),
  v2_x_response_schema!("2.0.1", "UnpublishFirmware"),
  v2_x_response_schema!("2.0.1", "UpdateFirmware"),
];

const V2_1_REQUEST_SCHEMAS: &[EmbeddedSchema] = &[
  v2_x_request_schema!("2.1", "AFRRSignal"),
  v2_x_request_schema!("2.1", "AdjustPeriodicEventStream"),
  v2_x_request_schema!("2.1", "Authorize"),
  v2_x_request_schema!("2.1", "BatterySwap"),
  v2_x_request_schema!("2.1", "BootNotification"),
  v2_x_request_schema!("2.1", "CancelReservation"),
  v2_x_request_schema!("2.1", "CertificateSigned"),
  v2_x_request_schema!("2.1", "ChangeAvailability"),
  v2_x_request_schema!("2.1", "ChangeTransactionTariff"),
  v2_x_request_schema!("2.1", "ClearCache"),
  v2_x_request_schema!("2.1", "ClearChargingProfile"),
  v2_x_request_schema!("2.1", "ClearDERControl"),
  v2_x_request_schema!("2.1", "ClearDisplayMessage"),
  v2_x_request_schema!("2.1", "ClearTariffs"),
  v2_x_request_schema!("2.1", "ClearVariableMonitoring"),
  v2_x_request_schema!("2.1", "ClearedChargingLimit"),
  v2_x_request_schema!("2.1", "ClosePeriodicEventStream"),
  v2_x_request_schema!("2.1", "CostUpdated"),
  v2_x_request_schema!("2.1", "CustomerInformation"),
  v2_x_request_schema!("2.1", "DataTransfer"),
  v2_x_request_schema!("2.1", "DeleteCertificate"),
  v2_x_request_schema!("2.1", "FirmwareStatusNotification"),
  v2_x_request_schema!("2.1", "Get15118EVCertificate"),
  v2_x_request_schema!("2.1", "GetBaseReport"),
  v2_x_request_schema!("2.1", "GetCertificateChainStatus"),
  v2_x_request_schema!("2.1", "GetCertificateStatus"),
  v2_x_request_schema!("2.1", "GetChargingProfiles"),
  v2_x_request_schema!("2.1", "GetCompositeSchedule"),
  v2_x_request_schema!("2.1", "GetDERControl"),
  v2_x_request_schema!("2.1", "GetDisplayMessages"),
  v2_x_request_schema!("2.1", "GetInstalledCertificateIds"),
  v2_x_request_schema!("2.1", "GetLocalListVersion"),
  v2_x_request_schema!("2.1", "GetLog"),
  v2_x_request_schema!("2.1", "GetMonitoringReport"),
  v2_x_request_schema!("2.1", "GetPeriodicEventStream"),
  v2_x_request_schema!("2.1", "GetReport"),
  v2_x_request_schema!("2.1", "GetTariffs"),
  v2_x_request_schema!("2.1", "GetTransactionStatus"),
  v2_x_request_schema!("2.1", "GetVariables"),
  v2_x_request_schema!("2.1", "Heartbeat"),
  v2_x_request_schema!("2.1", "InstallCertificate"),
  v2_x_request_schema!("2.1", "LogStatusNotification"),
  v2_x_request_schema!("2.1", "MeterValues"),
  v2_x_request_schema!("2.1", "NotifyAllowedEnergyTransfer"),
  v2_x_request_schema!("2.1", "NotifyChargingLimit"),
  v2_x_request_schema!("2.1", "NotifyCustomerInformation"),
  v2_x_request_schema!("2.1", "NotifyDERAlarm"),
  v2_x_request_schema!("2.1", "NotifyDERStartStop"),
  v2_x_request_schema!("2.1", "NotifyDisplayMessages"),
  v2_x_request_schema!("2.1", "NotifyEVChargingNeeds"),
  v2_x_request_schema!("2.1", "NotifyEVChargingSchedule"),
  v2_x_request_schema!("2.1", "NotifyEvent"),
  v2_x_request_schema!("2.1", "NotifyMonitoringReport"),
  v2_x_request_schema!(
    "2.1",
    "NotifyPeriodicEventStream",
    "NotifyPeriodicEventStream.json"
  ),
  v2_x_request_schema!("2.1", "NotifyPriorityCharging"),
  v2_x_request_schema!("2.1", "NotifyReport"),
  v2_x_request_schema!("2.1", "NotifySettlement"),
  v2_x_request_schema!("2.1", "NotifyWebPaymentStarted"),
  v2_x_request_schema!("2.1", "OpenPeriodicEventStream"),
  v2_x_request_schema!("2.1", "PublishFirmware"),
  v2_x_request_schema!("2.1", "PublishFirmwareStatusNotification"),
  v2_x_request_schema!("2.1", "PullDynamicScheduleUpdate"),
  v2_x_request_schema!("2.1", "ReportChargingProfiles"),
  v2_x_request_schema!("2.1", "ReportDERControl"),
  v2_x_request_schema!("2.1", "RequestBatterySwap"),
  v2_x_request_schema!("2.1", "RequestStartTransaction"),
  v2_x_request_schema!("2.1", "RequestStopTransaction"),
  v2_x_request_schema!("2.1", "ReservationStatusUpdate"),
  v2_x_request_schema!("2.1", "ReserveNow"),
  v2_x_request_schema!("2.1", "Reset"),
  v2_x_request_schema!("2.1", "SecurityEventNotification"),
  v2_x_request_schema!("2.1", "SendLocalList"),
  v2_x_request_schema!("2.1", "SetChargingProfile"),
  v2_x_request_schema!("2.1", "SetDERControl"),
  v2_x_request_schema!("2.1", "SetDefaultTariff"),
  v2_x_request_schema!("2.1", "SetDisplayMessage"),
  v2_x_request_schema!("2.1", "SetMonitoringBase"),
  v2_x_request_schema!("2.1", "SetMonitoringLevel"),
  v2_x_request_schema!("2.1", "SetNetworkProfile"),
  v2_x_request_schema!("2.1", "SetVariableMonitoring"),
  v2_x_request_schema!("2.1", "SetVariables"),
  v2_x_request_schema!("2.1", "SignCertificate"),
  v2_x_request_schema!("2.1", "StatusNotification"),
  v2_x_request_schema!("2.1", "TransactionEvent"),
  v2_x_request_schema!("2.1", "TriggerMessage"),
  v2_x_request_schema!("2.1", "UnlockConnector"),
  v2_x_request_schema!("2.1", "UnpublishFirmware"),
  v2_x_request_schema!("2.1", "UpdateDynamicSchedule"),
  v2_x_request_schema!("2.1", "UpdateFirmware"),
  v2_x_request_schema!("2.1", "UsePriorityCharging"),
  v2_x_request_schema!("2.1", "VatNumberValidation"),
];

const V2_1_RESPONSE_SCHEMAS: &[EmbeddedSchema] = &[
  v2_x_response_schema!("2.1", "AFRRSignal"),
  v2_x_response_schema!("2.1", "AdjustPeriodicEventStream"),
  v2_x_response_schema!("2.1", "Authorize"),
  v2_x_response_schema!("2.1", "BatterySwap"),
  v2_x_response_schema!("2.1", "BootNotification"),
  v2_x_response_schema!("2.1", "CancelReservation"),
  v2_x_response_schema!("2.1", "CertificateSigned"),
  v2_x_response_schema!("2.1", "ChangeAvailability"),
  v2_x_response_schema!("2.1", "ChangeTransactionTariff"),
  v2_x_response_schema!("2.1", "ClearCache"),
  v2_x_response_schema!("2.1", "ClearChargingProfile"),
  v2_x_response_schema!("2.1", "ClearDERControl"),
  v2_x_response_schema!("2.1", "ClearDisplayMessage"),
  v2_x_response_schema!("2.1", "ClearTariffs"),
  v2_x_response_schema!("2.1", "ClearVariableMonitoring"),
  v2_x_response_schema!("2.1", "ClearedChargingLimit"),
  v2_x_response_schema!("2.1", "ClosePeriodicEventStream"),
  v2_x_response_schema!("2.1", "CostUpdated"),
  v2_x_response_schema!("2.1", "CustomerInformation"),
  v2_x_response_schema!("2.1", "DataTransfer"),
  v2_x_response_schema!("2.1", "DeleteCertificate"),
  v2_x_response_schema!("2.1", "FirmwareStatusNotification"),
  v2_x_response_schema!("2.1", "Get15118EVCertificate"),
  v2_x_response_schema!("2.1", "GetBaseReport"),
  v2_x_response_schema!("2.1", "GetCertificateChainStatus"),
  v2_x_response_schema!("2.1", "GetCertificateStatus"),
  v2_x_response_schema!("2.1", "GetChargingProfiles"),
  v2_x_response_schema!("2.1", "GetCompositeSchedule"),
  v2_x_response_schema!("2.1", "GetDERControl"),
  v2_x_response_schema!("2.1", "GetDisplayMessages"),
  v2_x_response_schema!("2.1", "GetInstalledCertificateIds"),
  v2_x_response_schema!("2.1", "GetLocalListVersion"),
  v2_x_response_schema!("2.1", "GetLog"),
  v2_x_response_schema!("2.1", "GetMonitoringReport"),
  v2_x_response_schema!("2.1", "GetPeriodicEventStream"),
  v2_x_response_schema!("2.1", "GetReport"),
  v2_x_response_schema!("2.1", "GetTariffs"),
  v2_x_response_schema!("2.1", "GetTransactionStatus"),
  v2_x_response_schema!("2.1", "GetVariables"),
  v2_x_response_schema!("2.1", "Heartbeat"),
  v2_x_response_schema!("2.1", "InstallCertificate"),
  v2_x_response_schema!("2.1", "LogStatusNotification"),
  v2_x_response_schema!("2.1", "MeterValues"),
  v2_x_response_schema!("2.1", "NotifyAllowedEnergyTransfer"),
  v2_x_response_schema!("2.1", "NotifyChargingLimit"),
  v2_x_response_schema!("2.1", "NotifyCustomerInformation"),
  v2_x_response_schema!("2.1", "NotifyDERAlarm"),
  v2_x_response_schema!("2.1", "NotifyDERStartStop"),
  v2_x_response_schema!("2.1", "NotifyDisplayMessages"),
  v2_x_response_schema!("2.1", "NotifyEVChargingNeeds"),
  v2_x_response_schema!("2.1", "NotifyEVChargingSchedule"),
  v2_x_response_schema!("2.1", "NotifyEvent"),
  v2_x_response_schema!("2.1", "NotifyMonitoringReport"),
  v2_x_response_schema!("2.1", "NotifyPriorityCharging"),
  v2_x_response_schema!("2.1", "NotifyReport"),
  v2_x_response_schema!("2.1", "NotifySettlement"),
  v2_x_response_schema!("2.1", "NotifyWebPaymentStarted"),
  v2_x_response_schema!("2.1", "OpenPeriodicEventStream"),
  v2_x_response_schema!("2.1", "PublishFirmware"),
  v2_x_response_schema!("2.1", "PublishFirmwareStatusNotification"),
  v2_x_response_schema!("2.1", "PullDynamicScheduleUpdate"),
  v2_x_response_schema!("2.1", "ReportChargingProfiles"),
  v2_x_response_schema!("2.1", "ReportDERControl"),
  v2_x_response_schema!("2.1", "RequestBatterySwap"),
  v2_x_response_schema!("2.1", "RequestStartTransaction"),
  v2_x_response_schema!("2.1", "RequestStopTransaction"),
  v2_x_response_schema!("2.1", "ReservationStatusUpdate"),
  v2_x_response_schema!("2.1", "ReserveNow"),
  v2_x_response_schema!("2.1", "Reset"),
  v2_x_response_schema!("2.1", "SecurityEventNotification"),
  v2_x_response_schema!("2.1", "SendLocalList"),
  v2_x_response_schema!("2.1", "SetChargingProfile"),
  v2_x_response_schema!("2.1", "SetDERControl"),
  v2_x_response_schema!("2.1", "SetDefaultTariff"),
  v2_x_response_schema!("2.1", "SetDisplayMessage"),
  v2_x_response_schema!("2.1", "SetMonitoringBase"),
  v2_x_response_schema!("2.1", "SetMonitoringLevel"),
  v2_x_response_schema!("2.1", "SetNetworkProfile"),
  v2_x_response_schema!("2.1", "SetVariableMonitoring"),
  v2_x_response_schema!("2.1", "SetVariables"),
  v2_x_response_schema!("2.1", "SignCertificate"),
  v2_x_response_schema!("2.1", "StatusNotification"),
  v2_x_response_schema!("2.1", "TransactionEvent"),
  v2_x_response_schema!("2.1", "TriggerMessage"),
  v2_x_response_schema!("2.1", "UnlockConnector"),
  v2_x_response_schema!("2.1", "UnpublishFirmware"),
  v2_x_response_schema!("2.1", "UpdateDynamicSchedule"),
  v2_x_response_schema!("2.1", "UpdateFirmware"),
  v2_x_response_schema!("2.1", "UsePriorityCharging"),
  v2_x_response_schema!("2.1", "VatNumberValidation"),
];

#[cfg_attr(not(test), allow(dead_code))]
const ALL_SCHEMA_SETS: &[&[EmbeddedSchema]] = &[
  V1_6_REQUEST_SCHEMAS,
  V1_6_RESPONSE_SCHEMAS,
  V2_0_1_REQUEST_SCHEMAS,
  V2_0_1_RESPONSE_SCHEMAS,
  V2_1_REQUEST_SCHEMAS,
  V2_1_RESPONSE_SCHEMAS,
];

pub(crate) fn schemas(
  protocol: OcppVersion,
  schema_type: EmbeddedSchemaType,
) -> &'static [EmbeddedSchema] {
  match (protocol, schema_type) {
    (OcppVersion::V1_6, EmbeddedSchemaType::Request) => V1_6_REQUEST_SCHEMAS,
    (OcppVersion::V1_6, EmbeddedSchemaType::Response) => V1_6_RESPONSE_SCHEMAS,
    (OcppVersion::V2_0_1, EmbeddedSchemaType::Request) => {
      V2_0_1_REQUEST_SCHEMAS
    }
    (OcppVersion::V2_0_1, EmbeddedSchemaType::Response) => {
      V2_0_1_RESPONSE_SCHEMAS
    }
    (OcppVersion::V2_1, EmbeddedSchemaType::Request) => V2_1_REQUEST_SCHEMAS,
    (OcppVersion::V2_1, EmbeddedSchemaType::Response) => V2_1_RESPONSE_SCHEMAS,
  }
}

pub(crate) fn incoming_request_schemas(
  protocol: OcppVersion,
) -> &'static [EmbeddedSchema] {
  schemas(protocol, EmbeddedSchemaType::Request)
}

pub(crate) fn incoming_request_schema_text(
  protocol: OcppVersion,
  action: &str,
) -> Option<&'static str> {
  schema_text_for_action(incoming_request_schemas(protocol), action)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn schema_text(relative_path: &str) -> Option<&'static str> {
  ALL_SCHEMA_SETS.iter().find_map(|schemas| {
    schemas.iter().find_map(|schema| {
      (schema.relative_path == relative_path).then_some(schema.text)
    })
  })
}

fn schema_text_for_action(
  schemas: &[EmbeddedSchema],
  action: &str,
) -> Option<&'static str> {
  schemas
    .iter()
    .find_map(|schema| (schema.action == action).then_some(schema.text))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn embedded_schema_lookup_returns_known_actions() {
    let v1_6 = incoming_request_schema_text(OcppVersion::V1_6, "DataTransfer")
      .expect("v1.6 schema");
    assert!(v1_6.contains("\"vendorId\""));

    let v2_0_1 = incoming_request_schema_text(OcppVersion::V2_0_1, "Reset")
      .expect("v2.0.1 schema");
    assert!(v2_0_1.contains("\"type\""));

    let v2_1 = incoming_request_schema_text(
      OcppVersion::V2_1,
      "NotifyPeriodicEventStream",
    )
    .expect("v2.1 schema");
    assert!(v2_1.contains("\"basetime\""));

    let response = schema_text("schemas/2.0.1/GetVariablesResponse.json")
      .expect("embedded response schema");
    assert!(response.contains("\"getVariableResult\""));

    let unused_response = schema_text("schemas/2.1/AFRRSignalResponse.json")
      .expect("embedded unused response schema");
    assert!(unused_response.contains("\"$schema\""));

    assert!(
      incoming_request_schema_text(OcppVersion::V2_1, "TotallyUnknown")
        .is_none()
    );
  }

  #[test]
  fn embedded_schema_registry_covers_all_protocol_schema_sets() {
    assert_eq!(
      schemas(OcppVersion::V1_6, EmbeddedSchemaType::Request).len(),
      28
    );
    assert_eq!(
      schemas(OcppVersion::V1_6, EmbeddedSchemaType::Response).len(),
      28
    );
    assert_eq!(
      schemas(OcppVersion::V2_0_1, EmbeddedSchemaType::Request).len(),
      64
    );
    assert_eq!(
      schemas(OcppVersion::V2_0_1, EmbeddedSchemaType::Response).len(),
      64
    );
    assert_eq!(
      schemas(OcppVersion::V2_1, EmbeddedSchemaType::Request).len(),
      91
    );
    assert_eq!(
      schemas(OcppVersion::V2_1, EmbeddedSchemaType::Response).len(),
      90
    );
  }
}
