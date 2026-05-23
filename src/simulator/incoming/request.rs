#![allow(non_camel_case_types)]

use super::super::{
  ConnectorStatus, Result, Value, anyhow, normalize_identifier,
  optional_u16_field, required_i64_field, required_string_field,
  required_u16_field, required_u64_field,
};

#[derive(Debug, Clone)]
pub(in crate::simulator) struct RemoteStartTransactionRequestV1_6 {
  pub connector: Option<u16>,
  pub id_token: String,
}

impl RemoteStartTransactionRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      connector: optional_u16_field(payload, "connectorId")?,
      id_token: required_string_field(payload, "idTag")?.to_string(),
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct RemoteStopTransactionRequestV1_6 {
  pub transaction_id: i64,
}

impl RemoteStopTransactionRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      transaction_id: required_i64_field(payload, "transactionId")?,
    })
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct RequestStartTransactionRequest_V2_X {
  pub connector: Option<u16>,
  pub remote_start_id: i64,
  pub id_token: String,
}

impl RequestStartTransactionRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      connector: optional_u16_field(payload, "evseId")?,
      remote_start_id: required_i64_field(payload, "remoteStartId")?,
      id_token: required_nested_string_field(payload, "idToken", "idToken")?
        .to_string(),
    })
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct RequestStopTransactionRequest_V2_X {
  pub transaction_id: String,
}

impl RequestStopTransactionRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      transaction_id: required_string_field(payload, "transactionId")?
        .to_string(),
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct AvailabilityRequest {
  pub connector: Option<u16>,
  pub target_status: Option<ConnectorStatus>,
}

impl AvailabilityRequest {
  pub(in crate::simulator) fn parse_v1_6(payload: &Value) -> Result<Self> {
    let requested = required_string_field(payload, "type")?;
    Ok(Self {
      connector: optional_u16_field(payload, "connectorId")?,
      target_status: parse_operational_status(requested),
    })
  }

  pub(in crate::simulator) fn parse_v2_x(payload: &Value) -> Result<Self> {
    let requested = required_string_field(payload, "operationalStatus")?;
    Ok(Self {
      connector: optional_nested_u16_field(payload, "evse", "id")?,
      target_status: parse_operational_status(requested),
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct ReserveNowRequestV1_6 {
  pub connector: u16,
  pub reservation_id: i64,
}

impl ReserveNowRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    let _ = required_string_field(payload, "expiryDate")?;
    let _ = required_string_field(payload, "idTag")?;
    Ok(Self {
      connector: required_u16_field(payload, "connectorId")?,
      reservation_id: required_i64_field(payload, "reservationId")?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct ReserveNowRequest_V2_X {
  pub connector: Option<u16>,
  pub reservation_id: i64,
}

impl ReserveNowRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    let _ = required_string_field(payload, "expiryDateTime")?;
    let _ = required_nested_string_field(payload, "idToken", "idToken")?;
    let _ = required_nested_string_field(payload, "idToken", "type")?;
    Ok(Self {
      connector: optional_u16_field(payload, "evseId")?,
      reservation_id: required_i64_any_field(payload, &["id", "reservationId"])
        .ok_or_else(|| anyhow!("id is required."))?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct CancelReservationRequest {
  pub reservation_id: i64,
}

impl CancelReservationRequest {
  pub(in crate::simulator) fn parse_v1_6(payload: &Value) -> Result<Self> {
    Ok(Self {
      reservation_id: required_i64_field(payload, "reservationId")?,
    })
  }

  pub(in crate::simulator) fn parse_v2_x(payload: &Value) -> Result<Self> {
    Ok(Self {
      reservation_id: required_i64_any_field(payload, &["reservationId", "id"])
        .ok_or_else(|| anyhow!("reservationId is required."))?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct UnlockConnectorRequestV1_6 {
  pub connector: u16,
}

impl UnlockConnectorRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      connector: required_u16_field(payload, "connectorId")?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct UnlockConnectorRequest_V2_X {
  pub evse_id: u16,
  pub connector_id: u16,
}

impl UnlockConnectorRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      evse_id: required_u16_field(payload, "evseId")?,
      connector_id: required_u16_field(payload, "connectorId")?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct SendLocalListRequestV1_6 {
  pub list_version: i64,
}

impl SendLocalListRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    let _ = required_string_field(payload, "updateType")?;
    Ok(Self {
      list_version: required_i64_field(payload, "listVersion")?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct SendLocalListRequest_V2_X {
  pub version_number: i64,
}

impl SendLocalListRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    let _ = required_string_field(payload, "updateType")?;
    Ok(Self {
      version_number: required_i64_field(payload, "versionNumber")?,
    })
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct SetChargingProfileRequestV1_6 {
  pub connector: u16,
  pub profile: Value,
}

impl SetChargingProfileRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      connector: required_u16_field(payload, "connectorId")?,
      profile: required_object_field(payload, "csChargingProfiles")?,
    })
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct SetChargingProfileRequest_V2_X {
  pub connector: u16,
  pub profile: Value,
}

impl SetChargingProfileRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      connector: required_u16_field(payload, "evseId")?,
      profile: required_object_field(payload, "chargingProfile")?,
    })
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct CompositeScheduleRequestV1_6 {
  pub connector: u16,
  pub duration: u64,
  pub charging_rate_unit: Option<String>,
}

impl CompositeScheduleRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      connector: required_u16_field(payload, "connectorId")?,
      duration: required_u64_field(payload, "duration")?,
      charging_rate_unit: optional_string_field(payload, "chargingRateUnit")?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct CompositeScheduleRequest_V2_X {
  pub connector: u16,
  pub duration: u64,
}

impl CompositeScheduleRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      connector: required_u16_field(payload, "evseId")?,
      duration: required_u64_field(payload, "duration")?,
    })
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct TriggerMessageRequestV1_6 {
  pub requested_message: String,
  pub connector: Option<u16>,
}

impl TriggerMessageRequestV1_6 {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      requested_message: required_string_field(payload, "requestedMessage")?
        .to_string(),
      connector: optional_u16_field(payload, "connectorId")?,
    })
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct TriggerMessageRequest_V2_X {
  pub requested_message: String,
  pub connector: Option<u16>,
}

impl TriggerMessageRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    Ok(Self {
      requested_message: required_string_field(payload, "requestedMessage")?
        .to_string(),
      connector: optional_nested_u16_field(payload, "evse", "id")?,
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct UpdateFirmwareRequest_V2_X {
  pub request_id: i64,
}

impl UpdateFirmwareRequest_V2_X {
  pub(in crate::simulator) fn parse(payload: &Value) -> Result<Self> {
    let _ = required_nested_string_field(payload, "firmware", "location")?;
    let _ =
      required_nested_string_field(payload, "firmware", "retrieveDateTime")?;
    Ok(Self {
      request_id: required_i64_field(payload, "requestId")?,
    })
  }
}

fn parse_operational_status(value: &str) -> Option<ConnectorStatus> {
  let normalized = normalize_identifier(value);
  match normalized.as_str() {
    "operative" => Some(ConnectorStatus::Available),
    "inoperative" => Some(ConnectorStatus::Unavailable),
    _ => None,
  }
}

fn required_nested_string_field<'a>(
  payload: &'a Value,
  object_field: &str,
  field: &str,
) -> Result<&'a str> {
  payload
    .get(object_field)
    .and_then(Value::as_object)
    .and_then(|object| object.get(field))
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| anyhow!("{object_field}.{field} is required."))
}

fn required_i64_any_field(payload: &Value, fields: &[&str]) -> Option<i64> {
  fields
    .iter()
    .find_map(|field| payload.get(field).and_then(Value::as_i64))
}

fn optional_string_field(
  payload: &Value,
  field: &str,
) -> Result<Option<String>> {
  let Some(value) = payload.get(field) else {
    return Ok(None);
  };
  value
    .as_str()
    .map(|item| Some(item.to_string()))
    .ok_or_else(|| anyhow!("{field} must be a string."))
}

fn required_object_field(payload: &Value, field: &str) -> Result<Value> {
  let Some(value) = payload.get(field) else {
    return Err(anyhow!("{field} is required."));
  };
  if !value.is_object() {
    return Err(anyhow!("{field} must be an object."));
  }
  Ok(value.clone())
}

fn optional_nested_u16_field(
  payload: &Value,
  object_field: &str,
  field: &str,
) -> Result<Option<u16>> {
  let Some(value) = payload.get(object_field) else {
    return Ok(None);
  };
  let object = value
    .as_object()
    .ok_or_else(|| anyhow!("{object_field} must be an object."))?;
  let Some(value) = object.get(field) else {
    return Ok(None);
  };
  let raw = value.as_u64().ok_or_else(|| {
    anyhow!("{object_field}.{field} must be an unsigned integer.")
  })?;
  let parsed = u16::try_from(raw).map_err(|_| {
    anyhow!("{object_field}.{field} is outside the supported connector range.")
  })?;
  Ok(Some(parsed))
}
