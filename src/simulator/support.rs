use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use chrono::{SecondsFormat, Utc};
use serde_json::Value;

use crate::ocpp::{ConfigurationKey, OcppVersion, ResponseStatus, StopReason};

use super::{ConfigurationEntry, SimulatorConfig};

/// Builds the baseline OCPP configuration key map exposed by the simulator.
pub(in crate::simulator) fn default_configuration_entries(
  config: &SimulatorConfig,
) -> BTreeMap<ConfigurationKey, ConfigurationEntry> {
  let heartbeat_interval = config.heartbeat_seconds.unwrap_or(30).to_string();
  BTreeMap::from([
    (
      ConfigurationKey::AllowOfflineTxForUnknownId,
      ConfigurationEntry {
        value: "false".to_string(),
        read_only: false,
      },
    ),
    (
      ConfigurationKey::AuthorizeRemoteTxRequests,
      ConfigurationEntry {
        value: "true".to_string(),
        read_only: false,
      },
    ),
    (
      ConfigurationKey::HeartbeatInterval,
      ConfigurationEntry {
        value: heartbeat_interval,
        read_only: false,
      },
    ),
    (
      ConfigurationKey::MeterValueSampleInterval,
      ConfigurationEntry {
        value: "60".to_string(),
        read_only: false,
      },
    ),
    (
      ConfigurationKey::NumberOfConnectors,
      ConfigurationEntry {
        value: config.connectors.to_string(),
        read_only: true,
      },
    ),
    (
      ConfigurationKey::SupportedFeatureProfiles,
      ConfigurationEntry {
        value: "Core,FirmwareManagement,LocalAuthListManagement,\
SmartCharging,RemoteTrigger,Reservation"
          .to_string(),
        read_only: true,
      },
    ),
    (
      ConfigurationKey::WebSocketPingInterval,
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

/// Maps local stop reason text to an OCPP 1.6 stop reason value.
pub(in crate::simulator) fn map_stop_reason_v1_6(
  reason: Option<&str>,
  remote_stop: bool,
) -> StopReason {
  if remote_stop {
    return StopReason::Remote;
  }
  reason
    .and_then(StopReason::parse_user_input)
    .filter(|item| item.as_v1_6().is_some())
    .unwrap_or(StopReason::Local)
}

/// Maps local stop reason text to an OCPP 2.x stop reason value.
pub(in crate::simulator) fn map_stop_reason_v2_x(
  protocol: OcppVersion,
  reason: Option<&str>,
  remote_stop: bool,
) -> StopReason {
  if remote_stop {
    return StopReason::Remote;
  }
  reason
    .and_then(StopReason::parse_user_input)
    .filter(|item| item.as_v2_x(protocol).is_some())
    .unwrap_or(StopReason::Local)
}

/// Validates that the CSMS accepted the requested WebSocket subprotocol.
pub(in crate::simulator) fn validate_negotiated_subprotocol<'a>(
  expected: &str,
  negotiated: Option<&'a str>,
) -> Result<&'a str> {
  let Some(actual) = negotiated else {
    return Err(anyhow!(
      "CSMS did not negotiate required WebSocket subprotocol `{expected}`."
    ));
  };
  if actual != expected {
    return Err(anyhow!(
      "CSMS negotiated WebSocket subprotocol `{actual}` \
      but `{expected}` was requested."
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
    .ok_or_else(|| anyhow!("{field} is required."))
}

/// Reads a required integer field from an inbound request payload.
pub(in crate::simulator) fn required_i64_field(
  payload: &Value,
  field: &str,
) -> Result<i64> {
  payload
    .get(field)
    .and_then(Value::as_i64)
    .ok_or_else(|| anyhow!("{field} is required."))
}

/// Reads a required connector-like `u16` field from a request payload.
pub(in crate::simulator) fn required_u16_field(
  payload: &Value,
  field: &str,
) -> Result<u16> {
  optional_u16_field(payload, field)?.ok_or_else(|| {
    anyhow!("{field} is required and must be an unsigned integer.")
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
    .ok_or_else(|| anyhow!("{field} is required and must be unsigned."))
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
    .ok_or_else(|| anyhow!("{field} must be an unsigned integer."))?;
  let parsed = u16::try_from(raw).map_err(|_| {
    anyhow!("{field} is outside the supported connector range.")
  })?;
  Ok(Some(parsed))
}
