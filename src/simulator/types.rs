use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::args::ResolvedCliArgs;
use crate::ocpp::{
  ConfigurationKey, ConnectorStatus as OcppConnectorStatus, OcppVersion,
  StopReason, TransactionTriggerReason,
};

#[derive(Debug, Clone)]
pub struct SimulatorConfig {
  pub profile: Option<String>,
  pub ws_url: Option<String>,
  pub cp_id: Option<String>,
  pub protocol: OcppVersion,
  pub connectors: u16,
  pub vendor: String,
  pub model: String,
  pub firmware: String,
  pub append_cp_id: bool,
  pub trace_frames: bool,
  pub strict: bool,
  pub request_timeout: Duration,
  pub heartbeat_seconds: Option<u64>,
  pub outbound_queue_limit: usize,
  pub security_event_limit: usize,
  pub security_profile: Option<u8>,
  pub basic_auth_password: Option<String>,
  pub ca_cert_path: Option<PathBuf>,
  pub client_cert_path: Option<PathBuf>,
  pub client_key_path: Option<PathBuf>,
}

impl SimulatorConfig {
  /// Converts resolved CLI arguments into simulator runtime configuration.
  pub fn from_resolved(args: &ResolvedCliArgs) -> Self {
    Self {
      profile: args.profile.clone(),
      ws_url: args.ws_url.clone(),
      cp_id: args.cp_id.clone(),
      protocol: args.protocol,
      connectors: args.connectors,
      vendor: args.vendor.clone(),
      model: args.model.clone(),
      firmware: args.firmware.clone(),
      append_cp_id: args.append_cp_id,
      trace_frames: args.trace_frames,
      strict: args.strict,
      request_timeout: Duration::from_secs(args.request_timeout_seconds.max(5)),
      heartbeat_seconds: args.heartbeat_seconds,
      outbound_queue_limit: args.outbound_queue_limit,
      security_event_limit: args.security_event_limit,
      security_profile: args.security_profile,
      basic_auth_password: args.basic_auth_password.clone(),
      ca_cert_path: args.ca_cert_path.clone(),
      client_cert_path: args.client_cert_path.clone(),
      client_key_path: args.client_key_path.clone(),
    }
  }
}

#[derive(Debug, Clone)]
pub struct SimulatorConnectionConfig {
  pub profile: Option<String>,
  pub ws_url: String,
  pub cp_id: String,
  pub append_cp_id: bool,
  pub connectors: u16,
  pub protocol: OcppVersion,
  pub vendor: String,
  pub model: String,
  pub firmware: String,
  pub trace_frames: bool,
  pub strict: bool,
  pub request_timeout: Duration,
  pub heartbeat_seconds: Option<u64>,
  pub outbound_queue_limit: usize,
  pub security_event_limit: usize,
  pub security_profile: Option<u8>,
  pub basic_auth_password: Option<String>,
  pub ca_cert_path: Option<PathBuf>,
  pub client_cert_path: Option<PathBuf>,
  pub client_key_path: Option<PathBuf>,
}

impl SimulatorConnectionConfig {
  /// Converts resolved arguments into a connection target.
  ///
  /// Returns `None` for offline-only arguments without a WebSocket URL or
  /// charge point ID.
  pub fn from_resolved(args: &ResolvedCliArgs) -> Option<Self> {
    let ws_url = args.ws_url.clone()?;
    let cp_id = args.cp_id.clone()?;
    Some(Self {
      profile: args.profile.clone(),
      ws_url,
      cp_id,
      append_cp_id: args.append_cp_id,
      connectors: args.connectors,
      protocol: args.protocol,
      vendor: args.vendor.clone(),
      model: args.model.clone(),
      firmware: args.firmware.clone(),
      trace_frames: args.trace_frames,
      strict: args.strict,
      request_timeout: Duration::from_secs(args.request_timeout_seconds.max(5)),
      heartbeat_seconds: args.heartbeat_seconds,
      outbound_queue_limit: args.outbound_queue_limit,
      security_event_limit: args.security_event_limit,
      security_profile: args.security_profile,
      basic_auth_password: args.basic_auth_password.clone(),
      ca_cert_path: args.ca_cert_path.clone(),
      client_cert_path: args.client_cert_path.clone(),
      client_key_path: args.client_key_path.clone(),
    })
  }
}

#[derive(Debug, Clone, Copy)]
pub enum UiLogLevel {
  Info,
  Warn,
  Error,
  Tx,
  Rx,
}

impl UiLogLevel {
  /// Returns a short log-level label used in text output.
  pub fn label(self) -> &'static str {
    match self {
      Self::Info => "INFO",
      Self::Warn => "WARN",
      Self::Error => "ERROR",
      Self::Tx => "TX",
      Self::Rx => "RX",
    }
  }
}

#[derive(Debug, Clone)]
pub enum UiEvent {
  Log { level: UiLogLevel, message: String },
  RuntimeState(SimulatorRuntimeState),
  Snapshot(SimulatorSnapshot),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimulatorRuntimeState {
  pub connected: bool,
  pub queue_depth: usize,
  pub pending_action: Option<String>,
  pub active_transactions: usize,
  pub pending_reconnect: bool,
}

#[derive(Debug, Clone)]
pub struct SimulatorSnapshot {
  pub profile: Option<String>,
  pub cp_id: Option<String>,
  pub protocol: OcppVersion,
  pub connection_url: String,
  pub connected: bool,
  pub heartbeat_seconds: Option<u64>,
  pub queue_depth: usize,
  pub pending_action: Option<String>,
  pub connectors: Vec<ConnectorSnapshot>,
}

#[derive(Debug, Clone)]
pub struct ConnectorSnapshot {
  pub id: u16,
  pub status: String,
  pub meter_wh: i64,
  pub transaction: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SimulatorCommand {
  Connect {
    config: Option<Box<SimulatorConnectionConfig>>,
  },
  Disconnect,
  Status,
  Boot,
  Authorize {
    id_token: String,
  },
  DataTransfer {
    vendor_id: String,
    message_id: Option<String>,
    data: Option<String>,
  },
  StartTransaction {
    connector: u16,
    id_token: String,
  },
  StopTransaction {
    connector: u16,
    reason: Option<String>,
  },
  SetMeter {
    connector: u16,
    value_wh: i64,
  },
  SendMeter {
    connector: u16,
  },
  Heartbeat,
  StartHeartbeat {
    seconds: u64,
  },
  StopHeartbeat,
  SetConnectorStatus {
    connector: u16,
    status: String,
  },
  HeartbeatTick,
  Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::simulator) enum ConnectorStatus {
  Available,
  Preparing,
  Charging,
  SuspendedEvse,
  SuspendedEv,
  Finishing,
  Reserved,
  Unavailable,
  Faulted,
  Occupied,
}

impl ConnectorStatus {
  /// Returns the canonical internal display name for the connector status.
  pub(in crate::simulator) fn display(self) -> &'static str {
    match self {
      Self::Available => "Available",
      Self::Preparing => "Preparing",
      Self::Charging => "Charging",
      Self::SuspendedEvse => "SuspendedEVSE",
      Self::SuspendedEv => "SuspendedEV",
      Self::Finishing => "Finishing",
      Self::Reserved => "Reserved",
      Self::Unavailable => "Unavailable",
      Self::Faulted => "Faulted",
      Self::Occupied => "Occupied",
    }
  }

  /// Maps connector status to an OCPP 1.6 `StatusNotification.status` value.
  pub(in crate::simulator) fn as_v1_6(self) -> OcppConnectorStatus {
    match self {
      Self::Available => OcppConnectorStatus::Available,
      Self::Preparing => OcppConnectorStatus::Preparing,
      Self::Charging | Self::Occupied => OcppConnectorStatus::Charging,
      Self::SuspendedEvse => OcppConnectorStatus::SuspendedEvse,
      Self::SuspendedEv => OcppConnectorStatus::SuspendedEv,
      Self::Finishing => OcppConnectorStatus::Finishing,
      Self::Reserved => OcppConnectorStatus::Reserved,
      Self::Unavailable => OcppConnectorStatus::Unavailable,
      Self::Faulted => OcppConnectorStatus::Faulted,
    }
  }

  /// Maps connector status to an OCPP 2.x connector status value.
  pub(in crate::simulator) fn as_v2_x(self) -> OcppConnectorStatus {
    match self {
      Self::Available => OcppConnectorStatus::Available,
      Self::Reserved => OcppConnectorStatus::Reserved,
      Self::Unavailable => OcppConnectorStatus::Unavailable,
      Self::Faulted => OcppConnectorStatus::Faulted,
      Self::Occupied
      | Self::Preparing
      | Self::Charging
      | Self::SuspendedEvse
      | Self::SuspendedEv
      | Self::Finishing => OcppConnectorStatus::Occupied,
    }
  }

  /// Parses user or payload status text into a normalized enum value.
  pub(in crate::simulator) fn parse(input: &str) -> Option<Self> {
    let normalized = normalize_identifier(input);

    match normalized.as_str() {
      "available" => Some(Self::Available),
      "preparing" => Some(Self::Preparing),
      "charging" => Some(Self::Charging),
      "suspendedevse" => Some(Self::SuspendedEvse),
      "suspendedev" => Some(Self::SuspendedEv),
      "finishing" => Some(Self::Finishing),
      "reserved" => Some(Self::Reserved),
      "unavailable" => Some(Self::Unavailable),
      "faulted" => Some(Self::Faulted),
      "occupied" => Some(Self::Occupied),
      _ => None,
    }
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct ConnectorState {
  pub(in crate::simulator) status: ConnectorStatus,
  pub(in crate::simulator) meter_wh: i64,
  pub(in crate::simulator) offered_limit: Option<f64>,
  pub(in crate::simulator) scheduled_availability: Option<ConnectorStatus>,
  pub(in crate::simulator) transaction: Option<TransactionState>,
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct TransactionState {
  pub(in crate::simulator) local_id: u64,
  pub(in crate::simulator) transaction_uid: String,
  pub(in crate::simulator) id_token: String,
  pub(in crate::simulator) v1_6_transaction_id: Option<i64>,
  pub(in crate::simulator) remote_start_id: Option<i64>,
  pub(in crate::simulator) seq_no: u64,
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct QueuedCall {
  pub(in crate::simulator) action: String,
  pub(in crate::simulator) payload: Value,
  pub(in crate::simulator) context: PendingContext,
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct PendingCall {
  pub(in crate::simulator) message_id: String,
  pub(in crate::simulator) sent_at: Instant,
  pub(in crate::simulator) call: QueuedCall,
}

#[derive(Debug, Clone)]
pub(in crate::simulator) enum PendingContext {
  Boot,
  Heartbeat,
  DataTransfer,
  DiagnosticsStatusNotification,
  FirmwareStatusNotification,
  LogStatusNotification,
  SecurityEventNotification {
    event_id: u64,
  },
  SignCertificate,
  SignedFirmwareStatusNotification,
  Authorize {
    id_token: String,
  },
  RemoteStartAuthorizeV1_6 {
    connector: u16,
    id_token: String,
  },
  StatusNotification {
    connector: u16,
  },
  StartTxV1_6 {
    connector: u16,
    local_tx_id: u64,
  },
  StopTxV1_6 {
    connector: u16,
    local_tx_id: u64,
  },
  MeterValues {
    connector: u16,
  },
  TxEvent {
    connector: u16,
    local_tx_id: u64,
    event_type: TxEventType,
  },
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct SecurityState {
  pub(in crate::simulator) security_profile: Option<u8>,
  pub(in crate::simulator) basic_auth_password: Option<String>,
  pub(in crate::simulator) additional_root_certificate_check: bool,
  pub(in crate::simulator) certificate_store_max_length: usize,
  pub(in crate::simulator) certificate_signed_max_chain_size: usize,
  pub(in crate::simulator) cpo_name: String,
  pub(in crate::simulator) supported_file_transfer_protocols: Vec<String>,
  pub(in crate::simulator) certificates: Vec<InstalledCertificate>,
  pub(in crate::simulator) events: Vec<SecurityEvent>,
  pub(in crate::simulator) next_event_id: u64,
  pub(in crate::simulator) next_signing_request_id: i64,
  pub(in crate::simulator) pending_signing_request_ids: Vec<i64>,
  pub(in crate::simulator) pending_reconnect: Option<SecurityReconnectPlan>,
}

impl SecurityState {
  pub(in crate::simulator) fn new(config: &SimulatorConfig) -> Self {
    Self {
      security_profile: config.security_profile,
      basic_auth_password: config.basic_auth_password.clone(),
      additional_root_certificate_check: false,
      certificate_store_max_length: 10,
      certificate_signed_max_chain_size: 10_000,
      cpo_name: config.vendor.clone(),
      supported_file_transfer_protocols: vec![
        "HTTP".to_string(),
        "HTTPS".to_string(),
      ],
      certificates: Vec::new(),
      events: Vec::new(),
      next_event_id: 1,
      next_signing_request_id: 1,
      pending_signing_request_ids: Vec::new(),
      pending_reconnect: None,
    }
  }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::simulator) struct SecurityReconnectPlan {
  pub(in crate::simulator) fallback_security_profile: SecurityProfileFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::simulator) enum SecurityProfileFallback {
  None,
  Restore(Option<u8>),
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct InstalledCertificate {
  pub(in crate::simulator) certificate_type: String,
  pub(in crate::simulator) hash: CertificateHashData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::simulator) struct CertificateHashData {
  pub(in crate::simulator) hash_algorithm: String,
  pub(in crate::simulator) issuer_name_hash: String,
  pub(in crate::simulator) issuer_key_hash: String,
  pub(in crate::simulator) serial_number: String,
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct SecurityEvent {
  pub(in crate::simulator) id: u64,
  pub(in crate::simulator) event_type: String,
  pub(in crate::simulator) timestamp: String,
  pub(in crate::simulator) tech_info: Option<String>,
  pub(in crate::simulator) notification_state: SecurityEventNotificationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::simulator) enum SecurityEventNotificationState {
  Pending,
  Queued,
  Sent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::simulator) enum TxEventType {
  Started,
  Updated,
  Ended,
}

impl TxEventType {
  pub(in crate::simulator) fn as_str(self) -> &'static str {
    match self {
      Self::Started => "Started",
      Self::Updated => "Updated",
      Self::Ended => "Ended",
    }
  }

  // Kept for schema coverage and future inbound transaction event parsing.
  #[allow(dead_code)]
  pub(in crate::simulator) fn parse(value: &str) -> Option<Self> {
    match value {
      "Started" => Some(Self::Started),
      "Updated" => Some(Self::Updated),
      "Ended" => Some(Self::Ended),
      _ => None,
    }
  }
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct TransactionEventRequest {
  pub(in crate::simulator) connector: u16,
  pub(in crate::simulator) local_tx_id: u64,
  pub(in crate::simulator) event_type: TxEventType,
  pub(in crate::simulator) trigger_reason: TransactionTriggerReason,
  pub(in crate::simulator) id_token: Option<String>,
  pub(in crate::simulator) remote_start_id: Option<i64>,
  pub(in crate::simulator) stopped_reason: Option<StopReason>,
}

#[derive(Debug)]
pub(in crate::simulator) struct HeartbeatTask {
  pub(in crate::simulator) seconds: u64,
  pub(in crate::simulator) handle: JoinHandle<()>,
}

#[derive(Debug, Clone)]
pub(in crate::simulator) struct ConfigurationEntry {
  pub(in crate::simulator) value: String,
  pub(in crate::simulator) read_only: bool,
}

pub(in crate::simulator) struct Simulator {
  pub(in crate::simulator) config: SimulatorConfig,
  pub(in crate::simulator) ui_tx: UnboundedSender<UiEvent>,
  pub(in crate::simulator) self_cmd_tx: UnboundedSender<SimulatorCommand>,
  pub(in crate::simulator) connectors: BTreeMap<u16, ConnectorState>,
  pub(in crate::simulator) configuration:
    BTreeMap<ConfigurationKey, ConfigurationEntry>,
  pub(in crate::simulator) reservations: BTreeMap<i64, u16>,
  pub(in crate::simulator) charging_profiles: BTreeMap<u16, Value>,
  pub(in crate::simulator) security: SecurityState,
  pub(in crate::simulator) local_auth_list_version: i64,
  pub(in crate::simulator) queue: VecDeque<QueuedCall>,
  pub(in crate::simulator) pending: Option<PendingCall>,
  pub(in crate::simulator) next_message_id: u64,
  pub(in crate::simulator) next_tx_id: u64,
  pub(in crate::simulator) heartbeat: Option<HeartbeatTask>,
  pub(in crate::simulator) connected: bool,
}

/// Normalizes free-form identifier text for case-insensitive matching.
///
/// Strips non-alphanumeric characters and lowercases the remainder.
pub(in crate::simulator) fn normalize_identifier(text: &str) -> String {
  text
    .chars()
    .filter(char::is_ascii_alphanumeric)
    .map(|ch| ch.to_ascii_lowercase())
    .collect()
}
