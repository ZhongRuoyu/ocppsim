use serde_json::{Value, json};

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcppVersion {
  V1_6,
  V2_0_1,
  V2_1,
}

impl OcppVersion {
  /// Returns the standards label used in human-facing output.
  pub fn label(self) -> &'static str {
    match self {
      Self::V1_6 => "1.6",
      Self::V2_0_1 => "2.0.1",
      Self::V2_1 => "2.1",
    }
  }

  /// Returns the WebSocket subprotocol token for the selected version.
  pub fn subprotocol(self) -> &'static str {
    match self {
      Self::V1_6 => "ocpp1.6",
      Self::V2_0_1 => "ocpp2.0.1",
      Self::V2_1 => "ocpp2.1",
    }
  }
}

pub const OCPP_V1_6_SUPPORTED_ACTIONS: &[&str] = &[
  "Authorize",
  "BootNotification",
  "CancelReservation",
  "ChangeAvailability",
  "ChangeConfiguration",
  "ClearCache",
  "ClearChargingProfile",
  "DataTransfer",
  "DiagnosticsStatusNotification",
  "FirmwareStatusNotification",
  "GetCompositeSchedule",
  "GetConfiguration",
  "GetDiagnostics",
  "GetLocalListVersion",
  "Heartbeat",
  "MeterValues",
  "RemoteStartTransaction",
  "RemoteStopTransaction",
  "ReserveNow",
  "Reset",
  "SendLocalList",
  "SetChargingProfile",
  "StartTransaction",
  "StatusNotification",
  "StopTransaction",
  "TriggerMessage",
  "UnlockConnector",
  "UpdateFirmware",
];

pub const OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS: &[&str] = &[
  "CertificateSigned",
  "DeleteCertificate",
  "ExtendedTriggerMessage",
  "GetInstalledCertificateIds",
  "GetLog",
  "InstallCertificate",
  "LogStatusNotification",
  "SecurityEventNotification",
  "SignCertificate",
  "SignedFirmwareStatusNotification",
  "SignedUpdateFirmware",
];

pub const OCPP_V2_X_COMMON_SUPPORTED_ACTIONS: &[&str] = &[
  "Authorize",
  "BootNotification",
  "CancelReservation",
  "ChangeAvailability",
  "ClearCache",
  "ClearChargingProfile",
  "DataTransfer",
  "FirmwareStatusNotification",
  "GetCompositeSchedule",
  "GetLocalListVersion",
  "GetLog",
  "GetVariables",
  "Heartbeat",
  "LogStatusNotification",
  "MeterValues",
  "RequestStartTransaction",
  "RequestStopTransaction",
  "ReserveNow",
  "Reset",
  "SendLocalList",
  "SetChargingProfile",
  "SetVariables",
  "StatusNotification",
  "TransactionEvent",
  "TriggerMessage",
  "UnlockConnector",
  "UpdateFirmware",
];

pub const OCPP_V2_0_1_UNSUPPORTED_ACTIONS: &[&str] = &[
  "CertificateSigned",
  "ClearDisplayMessage",
  "ClearVariableMonitoring",
  "ClearedChargingLimit",
  "CostUpdated",
  "CustomerInformation",
  "DeleteCertificate",
  "Get15118EVCertificate",
  "GetBaseReport",
  "GetCertificateStatus",
  "GetChargingProfiles",
  "GetDisplayMessages",
  "GetInstalledCertificateIds",
  "GetMonitoringReport",
  "GetReport",
  "GetTransactionStatus",
  "InstallCertificate",
  "NotifyChargingLimit",
  "NotifyCustomerInformation",
  "NotifyDisplayMessages",
  "NotifyEVChargingNeeds",
  "NotifyEVChargingSchedule",
  "NotifyEvent",
  "NotifyMonitoringReport",
  "NotifyReport",
  "PublishFirmware",
  "PublishFirmwareStatusNotification",
  "ReportChargingProfiles",
  "ReservationStatusUpdate",
  "SecurityEventNotification",
  "SetDisplayMessage",
  "SetMonitoringBase",
  "SetMonitoringLevel",
  "SetNetworkProfile",
  "SetVariableMonitoring",
  "SignCertificate",
  "UnpublishFirmware",
];

pub const OCPP_V2_1_UNSUPPORTED_ACTIONS: &[&str] = &[
  "AFRRSignal",
  "AdjustPeriodicEventStream",
  "BatterySwap",
  "ChangeTransactionTariff",
  "ClearDERControl",
  "ClearTariffs",
  "ClosePeriodicEventStream",
  "GetCertificateChainStatus",
  "GetDERControl",
  "GetPeriodicEventStream",
  "GetTariffs",
  "NotifyAllowedEnergyTransfer",
  "NotifyDERAlarm",
  "NotifyDERStartStop",
  "NotifyPeriodicEventStream",
  "NotifyPriorityCharging",
  "NotifySettlement",
  "NotifyWebPaymentStarted",
  "OpenPeriodicEventStream",
  "PullDynamicScheduleUpdate",
  "ReportDERControl",
  "RequestBatterySwap",
  "SetDERControl",
  "SetDefaultTariff",
  "UpdateDynamicSchedule",
  "UsePriorityCharging",
  "VatNumberValidation",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcppMessageTypeId {
  Call,
  CallResult,
  CallError,
  CallResultError,
  Send,
}

impl OcppMessageTypeId {
  /// Parses a numeric OCPP `MessageTypeId` into a typed enum.
  pub fn from_i64(value: i64) -> Option<Self> {
    match value {
      2 => Some(Self::Call),
      3 => Some(Self::CallResult),
      4 => Some(Self::CallError),
      5 => Some(Self::CallResultError),
      6 => Some(Self::Send),
      _ => None,
    }
  }

  /// Returns the integer encoding for this OCPP `MessageTypeId`.
  pub fn value(self) -> i64 {
    match self {
      Self::Call => 2,
      Self::CallResult => 3,
      Self::CallError => 4,
      Self::CallResultError => 5,
      Self::Send => 6,
    }
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingAction_V1_6 {
  GetConfiguration,
  ChangeConfiguration,
  ClearCache,
  ChangeAvailability,
  DataTransfer,
  GetDiagnostics,
  UpdateFirmware,
  RemoteStartTransaction,
  RemoteStopTransaction,
  ReserveNow,
  CancelReservation,
  UnlockConnector,
  GetLocalListVersion,
  SendLocalList,
  SetChargingProfile,
  ClearChargingProfile,
  GetCompositeSchedule,
  TriggerMessage,
  Reset,
}

impl IncomingAction_V1_6 {
  /// Parses an incoming OCPP 1.6 action name.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "GetConfiguration" => Some(Self::GetConfiguration),
      "ChangeConfiguration" => Some(Self::ChangeConfiguration),
      "ClearCache" => Some(Self::ClearCache),
      "ChangeAvailability" => Some(Self::ChangeAvailability),
      "DataTransfer" => Some(Self::DataTransfer),
      "GetDiagnostics" => Some(Self::GetDiagnostics),
      "UpdateFirmware" => Some(Self::UpdateFirmware),
      "RemoteStartTransaction" => Some(Self::RemoteStartTransaction),
      "RemoteStopTransaction" => Some(Self::RemoteStopTransaction),
      "ReserveNow" => Some(Self::ReserveNow),
      "CancelReservation" => Some(Self::CancelReservation),
      "UnlockConnector" => Some(Self::UnlockConnector),
      "GetLocalListVersion" => Some(Self::GetLocalListVersion),
      "SendLocalList" => Some(Self::SendLocalList),
      "SetChargingProfile" => Some(Self::SetChargingProfile),
      "ClearChargingProfile" => Some(Self::ClearChargingProfile),
      "GetCompositeSchedule" => Some(Self::GetCompositeSchedule),
      "TriggerMessage" => Some(Self::TriggerMessage),
      "Reset" => Some(Self::Reset),
      _ => None,
    }
  }

  /// Returns true when an OCPP 1.6 action belongs to a known extension that
  /// is intentionally out of scope for the base-schema implementation.
  pub fn is_known_unsupported(value: &str) -> bool {
    OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS.contains(&value)
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingAction_V2_X {
  ChangeAvailability,
  ClearCache,
  DataTransfer,
  GetLocalListVersion,
  GetLog,
  GetVariables,
  RequestStartTransaction,
  RequestStopTransaction,
  ReserveNow,
  CancelReservation,
  SendLocalList,
  SetChargingProfile,
  SetVariables,
  ClearChargingProfile,
  GetCompositeSchedule,
  TriggerMessage,
  UnlockConnector,
  UpdateFirmware,
  Reset,
}

impl IncomingAction_V2_X {
  /// Parses an incoming OCPP 2.x action name.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "ChangeAvailability" => Some(Self::ChangeAvailability),
      "ClearCache" => Some(Self::ClearCache),
      "DataTransfer" => Some(Self::DataTransfer),
      "GetLocalListVersion" => Some(Self::GetLocalListVersion),
      "GetLog" => Some(Self::GetLog),
      "GetVariables" => Some(Self::GetVariables),
      "RequestStartTransaction" => Some(Self::RequestStartTransaction),
      "RequestStopTransaction" => Some(Self::RequestStopTransaction),
      "ReserveNow" => Some(Self::ReserveNow),
      "CancelReservation" => Some(Self::CancelReservation),
      "SendLocalList" => Some(Self::SendLocalList),
      "SetChargingProfile" => Some(Self::SetChargingProfile),
      "SetVariables" => Some(Self::SetVariables),
      "ClearChargingProfile" => Some(Self::ClearChargingProfile),
      "GetCompositeSchedule" => Some(Self::GetCompositeSchedule),
      "TriggerMessage" => Some(Self::TriggerMessage),
      "UnlockConnector" => Some(Self::UnlockConnector),
      "UpdateFirmware" => Some(Self::UpdateFirmware),
      "Reset" => Some(Self::Reset),
      _ => None,
    }
  }

  /// Returns true when an OCPP 2.x action is known but intentionally out of
  /// scope for the simulator's current 1.6-derived feature subset.
  pub fn is_known_unsupported(value: &str, version: OcppVersion) -> bool {
    match version {
      OcppVersion::V1_6 => false,
      OcppVersion::V2_0_1 => OCPP_V2_0_1_UNSUPPORTED_ACTIONS.contains(&value),
      OcppVersion::V2_1 => {
        OCPP_V2_0_1_UNSUPPORTED_ACTIONS.contains(&value)
          || OCPP_V2_1_UNSUPPORTED_ACTIONS.contains(&value)
      }
    }
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMessage_V1_6 {
  BootNotification,
  Heartbeat,
  MeterValues,
  StatusNotification,
}

impl TriggerMessage_V1_6 {
  /// Parses OCPP 1.6 `TriggerMessage` request variants.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "BootNotification" => Some(Self::BootNotification),
      "Heartbeat" => Some(Self::Heartbeat),
      "MeterValues" => Some(Self::MeterValues),
      "StatusNotification" => Some(Self::StatusNotification),
      _ => None,
    }
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMessage_V2_X {
  BootNotification,
  Heartbeat,
  MeterValues,
  StatusNotification,
}

impl TriggerMessage_V2_X {
  /// Parses OCPP 2.x `TriggerMessage` request variants.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "BootNotification" => Some(Self::BootNotification),
      "Heartbeat" => Some(Self::Heartbeat),
      "MeterValues" => Some(Self::MeterValues),
      "StatusNotification" => Some(Self::StatusNotification),
      _ => None,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseStatus {
  Accepted,
  Rejected,
  NotSupported,
  Unsupported,
  Scheduled,
  Occupied,
  Unavailable,
  UnlockFailed,
  Unlocked,
  Failed,
  Unknown,
  UnknownVendorId,
  Invalid,
  NotFound,
  UnknownConnector,
  OngoingAuthorizedTransaction,
  UnknownVariable,
}

impl ResponseStatus {
  /// Returns the wire-format status token used in OCPP payloads.
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Accepted => "Accepted",
      Self::Rejected => "Rejected",
      Self::NotSupported => "NotSupported",
      Self::Unsupported => "Unsupported",
      Self::Scheduled => "Scheduled",
      Self::Occupied => "Occupied",
      Self::Unavailable => "Unavailable",
      Self::UnlockFailed => "UnlockFailed",
      Self::Unlocked => "Unlocked",
      Self::Failed => "Failed",
      Self::Unknown => "Unknown",
      Self::UnknownVendorId => "UnknownVendorId",
      Self::Invalid => "Invalid",
      Self::NotFound => "NotFound",
      Self::UnknownConnector => "UnknownConnector",
      Self::OngoingAuthorizedTransaction => "OngoingAuthorizedTransaction",
      Self::UnknownVariable => "UnknownVariable",
    }
  }

  /// Parses a wire-format status token into the internal enum.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "Accepted" => Some(Self::Accepted),
      "Rejected" => Some(Self::Rejected),
      "NotSupported" => Some(Self::NotSupported),
      "Unsupported" => Some(Self::Unsupported),
      "Scheduled" => Some(Self::Scheduled),
      "Occupied" => Some(Self::Occupied),
      "Unavailable" => Some(Self::Unavailable),
      "UnlockFailed" => Some(Self::UnlockFailed),
      "Unlocked" => Some(Self::Unlocked),
      "Failed" => Some(Self::Failed),
      "Unknown" => Some(Self::Unknown),
      "UnknownVendorId" => Some(Self::UnknownVendorId),
      "Invalid" => Some(Self::Invalid),
      "NotFound" => Some(Self::NotFound),
      "UnknownConnector" => Some(Self::UnknownConnector),
      "OngoingAuthorizedTransaction" => {
        Some(Self::OngoingAuthorizedTransaction)
      }
      "UnknownVariable" => Some(Self::UnknownVariable),
      _ => None,
    }
  }
}

#[derive(Debug, Clone)]
pub enum OcppFrame {
  Call {
    message_id: String,
    action: String,
    payload: Value,
  },
  CallResult {
    message_id: String,
    payload: Value,
  },
  CallError {
    message_id: String,
    code: String,
    description: String,
    details: Value,
  },
  CallResultError {
    message_id: String,
    code: String,
    description: String,
    details: Value,
  },
  Send {
    message_id: String,
    action: String,
    payload: Value,
  },
  Unsupported {
    message_type: i64,
    message_id: Option<String>,
  },
}

/// Parses raw JSON text into a typed OCPP frame.
///
/// Expected input is an OCPP-J array frame. The function validates shape and
/// basic field types, and normalizes `null` payload objects to `{}`.
pub fn parse_frame(text: &str) -> Result<OcppFrame, String> {
  let value: Value =
    serde_json::from_str(text).map_err(|err| format!("Invalid JSON: {err}"))?;
  let array = value
    .as_array()
    .ok_or_else(|| "OCPP frame must be a JSON array.".to_string())?;
  if array.len() < 2 {
    return Err("OCPP frame is too short.".to_string());
  }

  let message_type = array[0]
    .as_i64()
    .ok_or_else(|| "MessageTypeId must be an integer.".to_string())?;
  let message_id = array[1].as_str().map(ToOwned::to_owned);

  match OcppMessageTypeId::from_i64(message_type) {
    Some(OcppMessageTypeId::Call) | Some(OcppMessageTypeId::Send) => {
      if array.len() != 4 {
        return Err(format!(
          "CALL/SEND frame must have 4 items, got {}.",
          array.len()
        ));
      }
      let parsed_id =
        message_id.ok_or_else(|| "MessageId must be a string.".to_string())?;
      let action = array[2]
        .as_str()
        .ok_or_else(|| "Action must be a string.".to_string())?
        .to_string();
      let payload = parse_payload_object(&array[3])?;
      if OcppMessageTypeId::from_i64(message_type)
        == Some(OcppMessageTypeId::Call)
      {
        Ok(OcppFrame::Call {
          message_id: parsed_id,
          action,
          payload,
        })
      } else {
        Ok(OcppFrame::Send {
          message_id: parsed_id,
          action,
          payload,
        })
      }
    }
    Some(OcppMessageTypeId::CallResult) => {
      if array.len() != 3 {
        return Err(format!(
          "CALLRESULT frame must have 3 items, got {}.",
          array.len()
        ));
      }
      let parsed_id =
        message_id.ok_or_else(|| "MessageId must be a string.".to_string())?;
      let payload = parse_payload_object(&array[2])?;
      Ok(OcppFrame::CallResult {
        message_id: parsed_id,
        payload,
      })
    }
    Some(OcppMessageTypeId::CallError)
    | Some(OcppMessageTypeId::CallResultError) => {
      if array.len() != 5 {
        return Err(format!(
          "CALLERROR frame must have 5 items, got {}.",
          array.len()
        ));
      }
      let parsed_id =
        message_id.ok_or_else(|| "MessageId must be a string.".to_string())?;
      let code = array[2]
        .as_str()
        .ok_or_else(|| "ErrorCode must be a string.".to_string())?
        .to_string();
      let description = array[3]
        .as_str()
        .ok_or_else(|| "ErrorDescription must be a string.".to_string())?
        .to_string();
      let details = parse_payload_object(&array[4])?;
      if OcppMessageTypeId::from_i64(message_type)
        == Some(OcppMessageTypeId::CallError)
      {
        Ok(OcppFrame::CallError {
          message_id: parsed_id,
          code,
          description,
          details,
        })
      } else {
        Ok(OcppFrame::CallResultError {
          message_id: parsed_id,
          code,
          description,
          details,
        })
      }
    }
    None => Ok(OcppFrame::Unsupported {
      message_type,
      message_id,
    }),
  }
}

/// Builds a CALL frame string (`[2, messageId, action, payload]`).
pub fn build_call(message_id: &str, action: &str, payload: Value) -> String {
  json!([OcppMessageTypeId::Call.value(), message_id, action, payload])
    .to_string()
}

/// Builds a CALLRESULT frame string (`[3, messageId, payload]`).
pub fn build_call_result(message_id: &str, payload: Value) -> String {
  json!([OcppMessageTypeId::CallResult.value(), message_id, payload])
    .to_string()
}

/// Builds a CALLERROR frame string (`[4, messageId, code, desc, details]`).
pub fn build_call_error(
  message_id: &str,
  code: &str,
  description: &str,
  details: Value,
) -> String {
  json!([
    OcppMessageTypeId::CallError.value(),
    message_id,
    code,
    description,
    details
  ])
  .to_string()
}

/// Validates and normalizes the payload object field of an OCPP frame.
///
/// `null` is accepted and converted to an empty object for convenience.
fn parse_payload_object(value: &Value) -> Result<Value, String> {
  if value.is_null() {
    return Ok(json!({}));
  }
  if !value.is_object() {
    return Err("Payload must be a JSON object.".to_string());
  }
  Ok(value.clone())
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;

  use serde_json::json;

  use super::{
    IncomingAction_V1_6, OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS,
    OCPP_V1_6_SUPPORTED_ACTIONS, OCPP_V2_0_1_UNSUPPORTED_ACTIONS,
    OCPP_V2_1_UNSUPPORTED_ACTIONS, OCPP_V2_X_COMMON_SUPPORTED_ACTIONS,
    OcppFrame, OcppVersion, build_call, parse_frame,
  };

  #[test]
  /// Verifies OCPP 1.6 base schema actions are all in the supported manifest.
  fn v1_6_manifest_covers_base_schema_actions() {
    let schema_actions = request_schema_actions(OcppVersion::V1_6);
    let supported = set_from_slice(OCPP_V1_6_SUPPORTED_ACTIONS);

    assert_eq!(schema_actions, supported);
  }

  #[test]
  /// Verifies OCPP 2.0.1 actions are explicitly supported or unsupported.
  fn v2_0_1_manifest_covers_schema_actions() {
    let schema_actions = request_schema_actions(OcppVersion::V2_0_1);
    let manifest = set_from_slices(&[
      OCPP_V2_X_COMMON_SUPPORTED_ACTIONS,
      OCPP_V2_0_1_UNSUPPORTED_ACTIONS,
    ]);

    assert_eq!(schema_actions, manifest);
  }

  #[test]
  /// Verifies OCPP 2.1 actions are explicitly supported or unsupported.
  fn v2_1_manifest_covers_schema_actions() {
    let schema_actions = request_schema_actions(OcppVersion::V2_1);
    let manifest = set_from_slices(&[
      OCPP_V2_X_COMMON_SUPPORTED_ACTIONS,
      OCPP_V2_0_1_UNSUPPORTED_ACTIONS,
      OCPP_V2_1_UNSUPPORTED_ACTIONS,
    ]);

    assert_eq!(schema_actions, manifest);
  }

  #[test]
  /// Verifies OCPP 1.6 security whitepaper actions are known out of scope.
  fn v1_6_security_extension_actions_are_not_supported() {
    for action in OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS {
      assert!(IncomingAction_V1_6::is_known_unsupported(action));
    }
  }

  #[test]
  /// Verifies OCPP-J CALL builders round-trip through frame parsing.
  fn call_builder_round_trips_through_parser() {
    let text = build_call("m1", "Heartbeat", json!({}));
    let frame = parse_frame(&text).expect("parse built call");

    let OcppFrame::Call {
      message_id,
      action,
      payload,
    } = frame
    else {
      panic!("expected CALL frame");
    };
    assert_eq!(message_id, "m1");
    assert_eq!(action, "Heartbeat");
    assert_eq!(payload, json!({}));
  }

  #[test]
  /// Verifies null payloads are normalized to empty objects.
  fn parser_normalizes_null_payload_objects() {
    let frame = parse_frame(r#"[3,"m1",null]"#).expect("parse call result");

    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload, json!({}));
  }

  #[test]
  /// Verifies malformed OCPP-J frames fail before dispatch.
  fn parser_rejects_invalid_frame_shapes() {
    assert!(parse_frame(r#"{"messageTypeId":2}"#).is_err());
    assert!(parse_frame(r#"[2,"m1","Heartbeat"]"#).is_err());
    assert!(parse_frame(r#"[3,"m1",[]]"#).is_err());
    assert!(parse_frame(r#"[4,"m1","Error",{},{}]"#).is_err());
  }

  fn request_schema_actions(protocol: OcppVersion) -> BTreeSet<String> {
    crate::embedded_schemas::incoming_request_schemas(protocol)
      .iter()
      .map(|schema| schema.action.to_string())
      .collect()
  }

  fn set_from_slice(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
  }

  fn set_from_slices(slices: &[&[&str]]) -> BTreeSet<String> {
    slices
      .iter()
      .flat_map(|items| items.iter().copied())
      .map(ToOwned::to_owned)
      .collect()
  }
}
