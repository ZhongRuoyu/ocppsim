// Typed payload structs for all outbound and response OCPP messages.
//
// Each struct derives `Serialize` so that field names are validated at
// compile time, making the wire-format shape self-documenting.

#![allow(non_camel_case_types)]

use serde::Serialize;
use serde_json::Value;

// ── Common nested types ────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChargingStationInfo<'a> {
  pub vendor_name: &'a str,
  pub model: &'a str,
  pub firmware_version: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IdTokenPayload<'a> {
  pub id_token: &'a str,
  #[serde(rename = "type")]
  pub token_type: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SampledValueV1_6<'a> {
  pub value: &'a str,
  pub context: &'a str,
  pub measurand: &'a str,
  pub unit: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UnitOfMeasure<'a> {
  pub unit: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SampledValue_V2_X<'a> {
  pub value: i64,
  pub context: &'a str,
  pub measurand: &'a str,
  pub unit_of_measure: UnitOfMeasure<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MeterValueEntry<'a, T: Serialize> {
  pub timestamp: &'a str,
  pub sampled_value: Vec<T>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TransactionInfoPayload<'a> {
  pub transaction_id: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub remote_start_id: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stopped_reason: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvsePayload {
  pub id: u16,
  pub connector_id: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChargingSchedulePeriod {
  pub start_period: i64,
  pub limit: f64,
}

// ── Outbound CALL payloads ─────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BootNotificationV1_6Request<'a> {
  pub charge_point_vendor: &'a str,
  pub charge_point_model: &'a str,
  pub firmware_version: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BootNotification_V2_X_Request<'a> {
  pub reason: &'a str,
  pub charging_station: ChargingStationInfo<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AuthorizeV1_6Request<'a> {
  pub id_tag: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Authorize_V2_X_Request<'a> {
  pub id_token: IdTokenPayload<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HeartbeatRequest {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DataTransferRequestPayload<'a> {
  pub vendor_id: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message_id: Option<&'a str>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub data: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatusPayload<'a> {
  pub status: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FirmwareStatus_V2_X_Payload<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub request_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LogStatusPayload<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub request_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SecurityEventNotificationRequest<'a> {
  #[serde(rename = "type")]
  pub event_type: &'a str,
  pub timestamp: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tech_info: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SignCertificateRequest<'a> {
  pub csr: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub certificate_type: Option<&'a str>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub request_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatusNotificationV1_6Request<'a> {
  pub connector_id: u16,
  pub error_code: &'a str,
  pub status: &'a str,
  pub timestamp: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatusNotification_V2_X_Request<'a> {
  pub timestamp: &'a str,
  pub connector_status: &'a str,
  pub evse_id: u16,
  pub connector_id: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MeterValuesV1_6Request<'a> {
  pub connector_id: u16,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub transaction_id: Option<i64>,
  pub meter_value: Vec<MeterValueEntry<'a, SampledValueV1_6<'a>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MeterValues_V2_X_Request<'a> {
  pub evse_id: u16,
  pub meter_value: Vec<MeterValueEntry<'a, SampledValue_V2_X<'a>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartTransactionV1_6Request<'a> {
  pub connector_id: u16,
  pub id_tag: &'a str,
  pub meter_start: i64,
  pub timestamp: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StopTransactionV1_6Request<'a> {
  pub transaction_id: i64,
  pub timestamp: &'a str,
  pub meter_stop: i64,
  pub id_tag: &'a str,
  pub reason: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TransactionEvent_V2_X_Request<'a> {
  pub event_type: &'a str,
  pub timestamp: &'a str,
  pub trigger_reason: &'a str,
  pub seq_no: u64,
  pub transaction_info: TransactionInfoPayload<'a>,
  pub evse: EvsePayload,
  pub meter_value: Vec<MeterValueEntry<'a, SampledValue_V2_X<'a>>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id_token: Option<IdTokenPayload<'a>>,
}

// ── Response payloads ──────────────────────────────────────────

#[derive(Serialize)]
pub(super) struct ConfigurationKeyEntry<'a> {
  pub key: &'a str,
  pub readonly: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub value: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetConfigurationV1_6Response<'a> {
  pub configuration_key: Vec<ConfigurationKeyEntry<'a>>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub unknown_key: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListVersionV1_6Response {
  pub list_version: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListVersion_V2_X_Response {
  pub version_number: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetDiagnosticsV1_6Response<'a> {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub file_name: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DataTransferResponse<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub data: Option<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatusResponse<'a> {
  pub status: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetLog_V2_X_Response<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub filename: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CertificateHashDataPayload<'a> {
  pub hash_algorithm: &'a str,
  pub issuer_name_hash: &'a str,
  pub issuer_key_hash: &'a str,
  pub serial_number: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CertificateHashDataChainPayload<'a> {
  pub certificate_type: &'a str,
  pub certificate_hash_data: CertificateHashDataPayload<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetInstalledCertificateIdsV1_6Response<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub certificate_hash_data: Vec<CertificateHashDataPayload<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetInstalledCertificateIds_V2_X_Response<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub certificate_hash_data_chain: Vec<CertificateHashDataChainPayload<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetVariables_V2_X_Response {
  pub get_variable_result: Vec<VariableResult_V2_X>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetVariables_V2_X_Response {
  pub set_variable_result: Vec<VariableResult_V2_X>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VariableResult_V2_X {
  pub attribute_status: &'static str,
  pub component: Value,
  pub variable: Value,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attribute_type: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attribute_value: Option<String>,
}

impl VariableResult_V2_X {
  /// Constructs a base result from a get/set variable entry.
  pub(super) fn from_entry(
    entry: &Value,
    status: &'static str,
  ) -> anyhow::Result<Self> {
    Ok(Self {
      attribute_status: status,
      component: entry
        .get("component")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("component is required."))?,
      variable: entry
        .get("variable")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("variable is required."))?,
      attribute_type: entry.get("attributeType").cloned(),
      attribute_value: None,
    })
  }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCompositeScheduleV1_6Response<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub connector_id: Option<u16>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub schedule_start: Option<&'a str>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub charging_schedule: Option<ChargingScheduleV1_6<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChargingScheduleV1_6<'a> {
  pub duration: u64,
  pub charging_rate_unit: &'a str,
  pub charging_schedule_period: Vec<ChargingSchedulePeriod>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCompositeSchedule_V2_X_Response<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub schedule: Option<CompositeSchedule_V2_X<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CompositeSchedule_V2_X<'a> {
  pub evse_id: u16,
  pub duration: u64,
  pub schedule_start: &'a str,
  pub charging_rate_unit: &'a str,
  pub charging_schedule_period: Vec<ChargingSchedulePeriod>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RequestStartTransactionResponse<'a> {
  pub status: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub transaction_id: Option<&'a str>,
}

/// Converts a `Serialize` value to `serde_json::Value`.
///
/// Panics only on serialization bugs (not possible for simple data
/// types), so calling code can treat this as infallible.
pub(super) fn to_value<T: Serialize>(payload: &T) -> Value {
  serde_json::to_value(payload).expect("payload serialization")
}
