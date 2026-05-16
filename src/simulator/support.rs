use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use chrono::{SecondsFormat, Utc};
use serde_json::Value;

use crate::ocpp::{OcppVersion, ResponseStatus};

use super::{ConfigurationEntry, SimulatorConfig, normalize_identifier};

/// Builds the baseline OCPP configuration key map exposed by the simulator.
pub(in crate::simulator) fn default_configuration_entries(
  config: &SimulatorConfig,
) -> BTreeMap<String, ConfigurationEntry> {
  let heartbeat_interval = config.heartbeat_seconds.unwrap_or(30).to_string();
  BTreeMap::from([
    (
      "AllowOfflineTxForUnknownId".to_string(),
      ConfigurationEntry {
        value: "false".to_string(),
        read_only: false,
      },
    ),
    (
      "AuthorizeRemoteTxRequests".to_string(),
      ConfigurationEntry {
        value: "true".to_string(),
        read_only: false,
      },
    ),
    (
      "HeartbeatInterval".to_string(),
      ConfigurationEntry {
        value: heartbeat_interval,
        read_only: false,
      },
    ),
    (
      "MeterValueSampleInterval".to_string(),
      ConfigurationEntry {
        value: "60".to_string(),
        read_only: false,
      },
    ),
    (
      "NumberOfConnectors".to_string(),
      ConfigurationEntry {
        value: config.connectors.to_string(),
        read_only: true,
      },
    ),
    (
      "SupportedFeatureProfiles".to_string(),
      ConfigurationEntry {
        value: "Core,FirmwareManagement,LocalAuthListManagement,\
SmartCharging,RemoteTrigger,Reservation"
          .to_string(),
        read_only: true,
      },
    ),
    (
      "WebSocketPingInterval".to_string(),
      ConfigurationEntry {
        value: "0".to_string(),
        read_only: false,
      },
    ),
  ])
}

/// Returns a UTC RFC3339 timestamp string for outbound OCPP payloads.
pub(in crate::simulator) fn now_timestamp() -> String {
  Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Extracts authorize response status from version-specific payload shapes.
pub(in crate::simulator) fn authorize_status(
  protocol: OcppVersion,
  payload: &Value,
) -> String {
  match protocol {
    OcppVersion::V1_6 => payload
      .get("idTagInfo")
      .and_then(Value::as_object)
      .and_then(|object| object.get("status"))
      .and_then(Value::as_str)
      .unwrap_or(ResponseStatus::Unknown.as_str())
      .to_string(),
    OcppVersion::V2_0_1 | OcppVersion::V2_1 => payload
      .get("idTokenInfo")
      .and_then(Value::as_object)
      .and_then(|object| object.get("status"))
      .and_then(Value::as_str)
      .unwrap_or(ResponseStatus::Unknown.as_str())
      .to_string(),
  }
}

/// Maps local stop reason text to OCPP 1.6 reason enumeration strings.
pub(in crate::simulator) fn map_stop_reason_v1_6(
  reason: Option<&str>,
  remote_stop: bool,
) -> &'static str {
  if remote_stop {
    return "Remote";
  }
  let normalized = normalize_identifier(reason.unwrap_or("local"));
  match normalized.as_str() {
    "emergencystop" => "EmergencyStop",
    "evdisconnected" => "EVDisconnected",
    "hardreset" => "HardReset",
    "powerloss" => "PowerLoss",
    "reboot" => "Reboot",
    "softreset" => "SoftReset",
    "unlockcommand" => "UnlockCommand",
    "deauthorized" => "DeAuthorized",
    "other" => "Other",
    _ => "Local",
  }
}

/// Maps local stop reason text to OCPP 2.x reason enumeration strings.
pub(in crate::simulator) fn map_stop_reason_v2_x(
  reason: Option<&str>,
  remote_stop: bool,
) -> &'static str {
  if remote_stop {
    return "Remote";
  }
  let normalized = normalize_identifier(reason.unwrap_or("local"));
  match normalized.as_str() {
    "deauthorized" => "DeAuthorized",
    "emergencystop" => "EmergencyStop",
    "energylimitreached" => "EnergyLimitReached",
    "evdisconnected" => "EVDisconnected",
    "groundfault" => "GroundFault",
    "immediatereset" => "ImmediateReset",
    "localoutofcredit" => "LocalOutOfCredit",
    "masterpass" => "MasterPass",
    "other" => "Other",
    "overcurrentfault" => "OvercurrentFault",
    "powerloss" => "PowerLoss",
    "powerquality" => "PowerQuality",
    "reboot" => "Reboot",
    "soclimitreached" => "SOCLimitReached",
    "stoppedbyev" => "StoppedByEV",
    "timelimitreached" => "TimeLimitReached",
    "timeout" => "Timeout",
    _ => "Local",
  }
}

/// Validates that the CSMS accepted the requested WebSocket subprotocol.
pub(in crate::simulator) fn validate_negotiated_subprotocol<'a>(
  expected: &str,
  negotiated: Option<&'a str>,
) -> Result<&'a str> {
  let Some(actual) = negotiated else {
    return Err(anyhow!(
      "CSMS did not negotiate required WebSocket subprotocol `{}`.",
      expected
    ));
  };
  if actual != expected {
    return Err(anyhow!(
      "CSMS negotiated WebSocket subprotocol `{}` but `{}` was requested.",
      actual,
      expected
    ));
  }
  Ok(actual)
}

/// Reads a required non-empty string field from an inbound request payload.
pub(in crate::simulator) fn required_string_field<'a>(
  payload: &'a Value,
  field: &str,
) -> Result<&'a str> {
  payload
    .get(field)
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| anyhow!("{} is required.", field))
}

/// Reads a required integer field from an inbound request payload.
pub(in crate::simulator) fn required_i64_field(
  payload: &Value,
  field: &str,
) -> Result<i64> {
  payload
    .get(field)
    .and_then(Value::as_i64)
    .ok_or_else(|| anyhow!("{} is required.", field))
}

/// Reads a required connector-like `u16` field from a request payload.
pub(in crate::simulator) fn required_u16_field(
  payload: &Value,
  field: &str,
) -> Result<u16> {
  optional_u16_field(payload, field)?.ok_or_else(|| {
    anyhow!("{} is required and must be an unsigned integer.", field)
  })
}

/// Reads a required unsigned integer field from an inbound request payload.
pub(in crate::simulator) fn required_u64_field(
  payload: &Value,
  field: &str,
) -> Result<u64> {
  payload
    .get(field)
    .and_then(Value::as_u64)
    .ok_or_else(|| anyhow!("{} is required and must be unsigned.", field))
}

/// Reads an optional connector-like `u16` field from a request payload.
pub(in crate::simulator) fn optional_u16_field(
  payload: &Value,
  field: &str,
) -> Result<Option<u16>> {
  let Some(value) = payload.get(field) else {
    return Ok(None);
  };
  let raw = value
    .as_u64()
    .ok_or_else(|| anyhow!("{} must be an unsigned integer.", field))?;
  let parsed = u16::try_from(raw).map_err(|_| {
    anyhow!("{} is outside the supported connector range.", field)
  })?;
  Ok(Some(parsed))
}
