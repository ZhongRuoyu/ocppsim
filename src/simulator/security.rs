use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use super::payloads::{
  CertificateHashDataChainPayload, CertificateHashDataPayload,
  GetInstalledCertificateIds_V2_X_Response,
  GetInstalledCertificateIdsV1_6Response, GetLog_V2_X_Response,
  SecurityEventNotificationRequest, SignCertificateRequest, StatusResponse,
  to_value,
};
use super::types::{
  CertificateHashData, InstalledCertificate, SecurityEvent,
  SecurityEventNotificationState,
};
use super::{
  CertificateType, OcppVersion, OutgoingAction, PendingContext, ResponseStatus,
  Result, Simulator, UiLogLevel, Value, anyhow, normalize_identifier,
  now_timestamp, required_i64_field, required_string_field,
};
use base64::Engine as _;
use http::HeaderValue;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore};
use tokio_tungstenite::Connector;
use url::Url;

use crate::sensitive::{redact_text_secrets, redact_url_secrets};

const SIMULATED_CSR: &str = concat!(
  "-----BEGIN CERTIFICATE REQUEST-----\n",
  "T0NQUFNJTS1TSU1VTEFURUQtQ1NS\n",
  "-----END CERTIFICATE REQUEST-----"
);
const MAX_INSTALL_CERTIFICATE_LENGTH: usize = 5_500;

impl Simulator {
  pub(in crate::simulator) fn validate_connection_security(
    &self,
    url: &Url,
  ) -> Result<()> {
    let Some(profile) = self.security.security_profile else {
      return Ok(());
    };
    let scheme = url.scheme();
    match profile {
      1 if scheme != "ws" => {
        return Err(anyhow!(
          "Security profile 1 requires a ws:// URL with HTTP Basic \
          authentication."
        ));
      }
      2 | 3 if scheme != "wss" => {
        return Err(anyhow!(
          "Security profile {profile} requires a wss:// URL."
        ));
      }
      1 | 2 => {
        if self.security.basic_auth_password.is_none() {
          return Err(anyhow!(
            "Security profile {profile} requires a basic auth password."
          ));
        }
        self.validated_basic_auth_identity(profile)?;
      }
      3 => {
        if self.config.client_cert_path.is_none()
          || self.config.client_key_path.is_none()
        {
          return Err(anyhow!(
            "Security profile 3 requires --client-cert and --client-key."
          ));
        }
      }
      _ => return Err(anyhow!("Unsupported security profile {profile}.")),
    }
    Ok(())
  }

  pub(in crate::simulator) fn basic_auth_header(
    &self,
  ) -> Result<Option<HeaderValue>> {
    let Some(profile) = self.security.security_profile else {
      return Ok(None);
    };
    if !matches!(profile, 1 | 2) {
      return Ok(None);
    }
    let Some(password) = self.security.basic_auth_password.as_deref() else {
      return Ok(None);
    };
    let cp_id = self.validated_basic_auth_identity(profile)?;
    let credentials = format!("{cp_id}:{password}");
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
    HeaderValue::from_str(&format!("Basic {encoded}"))
      .map(Some)
      .map_err(|error| anyhow!("Invalid Authorization header: {error}"))
  }

  pub(in crate::simulator) fn validated_basic_auth_identity(
    &self,
    profile: u8,
  ) -> Result<&str> {
    let Some(cp_id) = self.config.cp_id.as_deref() else {
      return Err(anyhow!(
        "Security profile {profile} requires a charge point id."
      ));
    };
    if cp_id.contains(':') {
      return Err(anyhow!(
        "Security profile {profile} requires a charge point id without `:` \
        for HTTP Basic authentication."
      ));
    }
    Ok(cp_id)
  }

  pub(in crate::simulator) fn tls_connector(
    &self,
  ) -> Result<Option<Connector>> {
    let use_custom_tls = self.config.ca_cert_path.is_some()
      || self.config.client_cert_path.is_some()
      || self.config.client_key_path.is_some();
    if !use_custom_tls {
      return Ok(None);
    }

    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if let Some(path) = self.config.ca_cert_path.as_deref() {
      let certs = load_certificates(path)?;
      let (added, ignored) = root_store.add_parsable_certificates(certs);
      if added == 0 {
        return Err(anyhow!(
          "No usable CA certificates found in {}.",
          path.display()
        ));
      }
      if ignored > 0 {
        self.log(
          UiLogLevel::Warn,
          format!(
            "Ignored {ignored} unparsable CA certificate(s) from {}.",
            path.display()
          ),
        );
      }
    }

    let builder = ClientConfig::builder().with_root_certificates(root_store);
    let config = match (
      self.config.client_cert_path.as_deref(),
      self.config.client_key_path.as_deref(),
    ) {
      (Some(cert_path), Some(key_path)) => builder
        .with_client_auth_cert(
          load_certificates(cert_path)?,
          load_private_key(key_path)?,
        )
        .map_err(|error| anyhow!("Invalid client certificate/key: {error}"))?,
      (None, None) => builder.with_no_client_auth(),
      _ => {
        return Err(anyhow!(
          "Client certificate authentication requires both --client-cert and \
          --client-key."
        ));
      }
    };
    Ok(Some(Connector::Rustls(Arc::new(config))))
  }

  pub(in crate::simulator) fn record_secure_connection_failure(
    &mut self,
    error: &dyn std::fmt::Display,
  ) {
    if matches!(self.security.security_profile, Some(2 | 3)) {
      self.record_security_event(
        "InvalidCentralSystemCertificate",
        Some(format!("Secure connection setup failed: {error}")),
      );
    }
  }

  pub(in crate::simulator) fn record_security_event(
    &mut self,
    event_type: &str,
    tech_info: Option<String>,
  ) {
    let event_id = self.security.next_event_id;
    self.security.next_event_id += 1;
    let timestamp = now_timestamp();
    self.security.events.push(SecurityEvent {
      id: event_id,
      event_type: event_type.to_string(),
      timestamp,
      tech_info,
      notification_state: SecurityEventNotificationState::Pending,
    });
    self.enqueue_pending_security_event_notifications();
  }

  pub(in crate::simulator) fn enqueue_pending_security_event_notifications(
    &mut self,
  ) {
    if !self.connected {
      return;
    }
    let event_ids = self
      .security
      .events
      .iter()
      .filter(|event| {
        event.notification_state == SecurityEventNotificationState::Pending
      })
      .map(|event| event.id)
      .collect::<Vec<_>>();
    for event_id in event_ids {
      self.enqueue_security_event_notification(event_id);
    }
  }

  pub(in crate::simulator) fn reset_inflight_security_event_notifications(
    &mut self,
  ) {
    for event in &mut self.security.events {
      if event.notification_state == SecurityEventNotificationState::Queued {
        event.notification_state = SecurityEventNotificationState::Pending;
      }
    }
  }

  pub(in crate::simulator) fn mark_security_event_notification_sent(
    &mut self,
    event_id: u64,
  ) {
    if let Some(event) = self
      .security
      .events
      .iter_mut()
      .find(|event| event.id == event_id)
    {
      event.notification_state = SecurityEventNotificationState::Sent;
    }
  }

  pub(in crate::simulator) fn retry_security_event_notification(
    &mut self,
    event_id: u64,
  ) {
    if let Some(event) = self
      .security
      .events
      .iter_mut()
      .find(|event| event.id == event_id)
      && event.notification_state != SecurityEventNotificationState::Sent
    {
      event.notification_state = SecurityEventNotificationState::Pending;
    }
  }

  fn enqueue_security_event_notification(&mut self, event_id: u64) {
    let Some((event_type, timestamp, tech_info)) = self
      .security
      .events
      .iter()
      .find(|event| event.id == event_id)
      .map(|event| {
        (
          event.event_type.clone(),
          event.timestamp.clone(),
          event.tech_info.clone(),
        )
      })
    else {
      return;
    };
    let payload = to_value(&SecurityEventNotificationRequest {
      event_type: &event_type,
      timestamp: &timestamp,
      tech_info: tech_info.as_deref(),
    });
    self.enqueue_call(
      OutgoingAction::SecurityEventNotification.as_str(),
      payload,
      PendingContext::SecurityEventNotification { event_id },
    );
    if let Some(event) = self
      .security
      .events
      .iter_mut()
      .find(|event| event.id == event_id)
    {
      event.notification_state = SecurityEventNotificationState::Queued;
    }
  }

  pub(in crate::simulator) fn enqueue_sign_certificate(
    &mut self,
    certificate_type: Option<&str>,
  ) {
    let request_id = if self.config.protocol == OcppVersion::V2_1 {
      let id = self.security.next_signing_request_id;
      self.security.next_signing_request_id += 1;
      self.security.pending_signing_request_ids.push(id);
      Some(id)
    } else {
      None
    };
    let payload = to_value(&SignCertificateRequest {
      csr: SIMULATED_CSR,
      certificate_type,
      request_id,
    });
    self.enqueue_call(
      OutgoingAction::SignCertificate.as_str(),
      payload,
      PendingContext::SignCertificate,
    );
  }

  pub(in crate::simulator) fn install_certificate_from_payload(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let certificate_type = required_string_field(payload, "certificateType")?;
    let certificate = required_string_field(payload, "certificate")?;
    if certificate.len() > MAX_INSTALL_CERTIFICATE_LENGTH {
      return Ok(ResponseStatus::Rejected);
    }
    Ok(self.install_certificate(certificate_type, certificate))
  }

  pub(in crate::simulator) fn delete_certificate_from_payload(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let hash = parse_certificate_hash_data(
      payload
        .get("certificateHashData")
        .ok_or_else(|| anyhow!("certificateHashData is required."))?,
    )?;
    let before = self.security.certificates.len();
    self.security.certificates.retain(|item| item.hash != hash);
    if self.security.certificates.len() == before {
      Ok(ResponseStatus::NotFound)
    } else {
      Ok(ResponseStatus::Accepted)
    }
  }

  pub(in crate::simulator) fn get_installed_certificate_ids_v1_6(
    &self,
    payload: &Value,
  ) -> Result<Value> {
    let certificate_type = required_string_field(payload, "certificateType")?;
    let certificates = self.certificates_of_type(certificate_type);
    let status = if certificates.is_empty() {
      ResponseStatus::NotFound
    } else {
      ResponseStatus::Accepted
    };
    let certificate_hash_data = certificates
      .iter()
      .map(|item| certificate_hash_payload(&item.hash))
      .collect::<Vec<_>>();
    Ok(to_value(&GetInstalledCertificateIdsV1_6Response {
      status: status.as_str(),
      certificate_hash_data,
    }))
  }

  pub(in crate::simulator) fn get_installed_certificate_ids_v2_x(
    &self,
    payload: &Value,
  ) -> Result<Value> {
    let certificate_type = required_string_field(payload, "certificateType")?;
    let certificates = self.certificates_of_type(certificate_type);
    let status = if certificates.is_empty() {
      ResponseStatus::NotFound
    } else {
      ResponseStatus::Accepted
    };
    let certificate_hash_data_chain = certificates
      .iter()
      .map(|item| CertificateHashDataChainPayload {
        certificate_type: &item.certificate_type,
        certificate_hash_data: certificate_hash_payload(&item.hash),
      })
      .collect::<Vec<_>>();
    Ok(to_value(&GetInstalledCertificateIds_V2_X_Response {
      status: status.as_str(),
      certificate_hash_data_chain,
    }))
  }

  pub(in crate::simulator) fn certificate_signed_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    self.certificate_signed(
      payload,
      CertificateType::ChargePointCertificate.as_str(),
      "InvalidChargePointCertificate",
    )
  }

  pub(in crate::simulator) fn certificate_signed_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    if self.config.protocol == OcppVersion::V2_1
      && let Some(request_id) = payload.get("requestId").and_then(Value::as_i64)
    {
      if !self
        .security
        .pending_signing_request_ids
        .contains(&request_id)
      {
        return Ok(ResponseStatus::Rejected);
      }
      self
        .security
        .pending_signing_request_ids
        .retain(|item| *item != request_id);
    }
    let certificate_type = payload
      .get("certificateType")
      .and_then(Value::as_str)
      .unwrap_or(CertificateType::ChargingStationCertificate.as_str());
    self.certificate_signed(
      payload,
      certificate_type,
      "InvalidChargingStationCertificate",
    )
  }

  pub(in crate::simulator) fn get_log_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<Value> {
    let request_id = required_i64_field(payload, "requestId")?;
    let log_type = required_string_field(payload, "logType")?;
    let location = payload
      .get("log")
      .and_then(Value::as_object)
      .and_then(|log| log.get("remoteLocation"))
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("log.remoteLocation is required."))?;
    self.log(
      UiLogLevel::Info,
      format!(
        "Received {log_type} GetLog request for {}",
        redact_url_secrets(location)
      ),
    );
    if !self.supports_file_transfer_location(location) {
      return Ok(to_value(&GetLog_V2_X_Response {
        status: ResponseStatus::Rejected.as_str(),
        filename: None,
      }));
    }
    if let Some(event) = self.security.events.last() {
      let detail = event
        .tech_info
        .as_deref()
        .map(|info| format!(" ({info})"))
        .unwrap_or_default();
      self.log(
        UiLogLevel::Info,
        format!(
          "Including {} recorded security event(s) in log export; \
            latest={} at {}{}.",
          self.security.events.len(),
          event.event_type,
          event.timestamp,
          redact_text_secrets(&detail)
        ),
      );
    }
    self.enqueue_log_status_notification(
      ResponseStatus::Uploading.as_str(),
      Some(request_id),
    );
    self.enqueue_log_status_notification(
      ResponseStatus::Uploaded.as_str(),
      Some(request_id),
    );
    let cp_id = self.config.cp_id.as_deref().unwrap_or("unknown");
    let filename = if normalize_identifier(log_type) == "securitylog" {
      format!("security-{cp_id}.log")
    } else {
      format!("log-{cp_id}.txt")
    };
    Ok(to_value(&GetLog_V2_X_Response {
      status: ResponseStatus::Accepted.as_str(),
      filename: Some(&filename),
    }))
  }

  pub(in crate::simulator) fn signed_update_firmware_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request_id = required_i64_field(payload, "requestId")?;
    let firmware = payload
      .get("firmware")
      .and_then(Value::as_object)
      .ok_or_else(|| anyhow!("firmware is required."))?;
    let location = firmware
      .get("location")
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("firmware.location is required."))?;
    let _ = firmware
      .get("retrieveDateTime")
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("firmware.retrieveDateTime is required."))?;
    let signing_certificate = firmware
      .get("signingCertificate")
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("firmware.signingCertificate is required."))?;
    let signature = firmware
      .get("signature")
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("firmware.signature is required."))?;

    if !self.supports_file_transfer_location(location) {
      return Ok(ResponseStatus::Rejected);
    }
    if is_simulated_invalid(signing_certificate) {
      self.record_security_event(
        "InvalidFirmwareSigningCertificate",
        Some("SignedUpdateFirmware signing certificate rejected".to_string()),
      );
      return Ok(ResponseStatus::InvalidCertificate);
    }

    self.log(
      UiLogLevel::Info,
      format!(
        "Received SignedUpdateFirmware request from {}",
        redact_url_secrets(location)
      ),
    );
    if is_simulated_invalid(signature) {
      self.enqueue_signed_firmware_status_notification(
        ResponseStatus::InvalidSignature.as_str(),
        Some(request_id),
      );
      self.record_security_event(
        "InvalidFirmwareSignature",
        Some("SignedUpdateFirmware signature rejected".to_string()),
      );
      return Ok(ResponseStatus::Accepted);
    }

    for status in [
      ResponseStatus::Downloading,
      ResponseStatus::Downloaded,
      ResponseStatus::SignatureVerified,
      ResponseStatus::Installing,
      ResponseStatus::Installed,
    ] {
      self.enqueue_signed_firmware_status_notification(
        status.as_str(),
        Some(request_id),
      );
    }
    self.record_security_event("FirmwareUpdated", None);
    Ok(ResponseStatus::Accepted)
  }

  pub(in crate::simulator) fn secure_update_firmware_v2_x(
    &mut self,
    request_id: i64,
    firmware: &Value,
  ) -> Result<ResponseStatus> {
    let location = firmware
      .get("location")
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("firmware.location is required."))?;
    if !self.supports_file_transfer_location(location) {
      return Ok(ResponseStatus::Rejected);
    }

    let signing_certificate =
      firmware.get("signingCertificate").and_then(Value::as_str);
    let signature = firmware.get("signature").and_then(Value::as_str);
    if signing_certificate.is_some_and(is_simulated_invalid) {
      self.record_security_event(
        "InvalidFirmwareSigningCertificate",
        Some("UpdateFirmware signing certificate rejected".to_string()),
      );
      return Ok(ResponseStatus::InvalidCertificate);
    }
    if signature.is_some_and(is_simulated_invalid) {
      self.record_security_event(
        "InvalidFirmwareSignature",
        Some("UpdateFirmware signature rejected".to_string()),
      );
      self.enqueue_firmware_status_notification(
        ResponseStatus::InvalidSignature.as_str(),
        Some(request_id),
      );
      return Ok(ResponseStatus::Accepted);
    }

    self.enqueue_firmware_status_notification(
      ResponseStatus::Downloading.as_str(),
      Some(request_id),
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Downloaded.as_str(),
      Some(request_id),
    );
    if signing_certificate.is_some() || signature.is_some() {
      self.enqueue_firmware_status_notification(
        ResponseStatus::SignatureVerified.as_str(),
        Some(request_id),
      );
    }
    self.enqueue_firmware_status_notification(
      ResponseStatus::Installing.as_str(),
      Some(request_id),
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Installed.as_str(),
      Some(request_id),
    );
    self.record_security_event("FirmwareUpdated", None);
    Ok(ResponseStatus::Accepted)
  }

  pub(in crate::simulator) fn signed_update_firmware_response(
    status: ResponseStatus,
  ) -> Value {
    to_value(&StatusResponse {
      status: status.as_str(),
    })
  }

  fn certificate_signed(
    &mut self,
    payload: &Value,
    certificate_type: &str,
    invalid_event_type: &str,
  ) -> Result<ResponseStatus> {
    let certificate_chain = required_string_field(payload, "certificateChain")?;
    if certificate_chain.len() > self.security.certificate_signed_max_chain_size
      || is_simulated_invalid(certificate_chain)
    {
      self.record_security_event(
        invalid_event_type,
        Some("CertificateSigned certificate chain rejected".to_string()),
      );
      return Ok(ResponseStatus::Rejected);
    }
    Ok(self.install_certificate(certificate_type, certificate_chain))
  }

  fn install_certificate(
    &mut self,
    certificate_type: &str,
    certificate: &str,
  ) -> ResponseStatus {
    if certificate.trim().is_empty() || is_simulated_invalid(certificate) {
      return ResponseStatus::Rejected;
    }
    let hash = synthetic_certificate_hash(certificate_type, certificate);
    if self
      .security
      .certificates
      .iter()
      .any(|item| item.hash == hash)
    {
      return ResponseStatus::Accepted;
    }
    if self.security.additional_root_certificate_check
      && is_central_system_root_certificate_type(certificate_type)
    {
      return self.install_central_system_root_with_fallback(
        certificate_type,
        certificate,
        hash,
      );
    }
    if self.security.certificates.len()
      >= self.security.certificate_store_max_length
    {
      return ResponseStatus::Rejected;
    }
    self.security.certificates.push(InstalledCertificate {
      certificate_type: certificate_type.to_string(),
      hash,
    });
    ResponseStatus::Accepted
  }

  fn install_central_system_root_with_fallback(
    &mut self,
    certificate_type: &str,
    certificate: &str,
    hash: CertificateHashData,
  ) -> ResponseStatus {
    if is_simulated_invalid(certificate) {
      return ResponseStatus::Rejected;
    }
    let fallback = self
      .security
      .certificates
      .iter()
      .rev()
      .find(|item| {
        is_central_system_root_certificate_type(&item.certificate_type)
      })
      .cloned();
    self.security.certificates.retain(|item| {
      !is_central_system_root_certificate_type(&item.certificate_type)
    });
    if let Some(fallback) = fallback {
      self.security.certificates.push(fallback);
    }
    if self.security.certificates.len()
      >= self.security.certificate_store_max_length
    {
      return ResponseStatus::Rejected;
    }
    self.security.certificates.push(InstalledCertificate {
      certificate_type: certificate_type.to_string(),
      hash,
    });
    ResponseStatus::Accepted
  }

  fn certificates_of_type(
    &self,
    certificate_type: &str,
  ) -> Vec<&InstalledCertificate> {
    self
      .security
      .certificates
      .iter()
      .filter(|item| item.certificate_type == certificate_type)
      .collect()
  }

  pub(in crate::simulator) fn supports_file_transfer_location(
    &self,
    location: &str,
  ) -> bool {
    let Some((scheme, _)) = location.split_once("://") else {
      return false;
    };
    self
      .security
      .supported_file_transfer_protocols
      .iter()
      .any(|item| item.eq_ignore_ascii_case(scheme))
  }
}

fn certificate_hash_payload(
  hash: &CertificateHashData,
) -> CertificateHashDataPayload<'_> {
  CertificateHashDataPayload {
    hash_algorithm: &hash.hash_algorithm,
    issuer_name_hash: &hash.issuer_name_hash,
    issuer_key_hash: &hash.issuer_key_hash,
    serial_number: &hash.serial_number,
  }
}

fn parse_certificate_hash_data(payload: &Value) -> Result<CertificateHashData> {
  Ok(CertificateHashData {
    hash_algorithm: required_string_field(payload, "hashAlgorithm")?
      .to_string(),
    issuer_name_hash: required_string_field(payload, "issuerNameHash")?
      .to_string(),
    issuer_key_hash: required_string_field(payload, "issuerKeyHash")?
      .to_string(),
    serial_number: required_string_field(payload, "serialNumber")?.to_string(),
  })
}

fn synthetic_certificate_hash(
  certificate_type: &str,
  certificate: &str,
) -> CertificateHashData {
  let seed = format!("{certificate_type}:{certificate}");
  let issuer_name_hash = synthetic_hex(&seed, 0x9e37_79b9_7f4a_7c15);
  let issuer_key_hash = synthetic_hex(&seed, 0xc2b2_ae3d_27d4_eb4f);
  let serial_number = synthetic_hex(&seed, 0x1656_67b1_9e37_79f9)
    .chars()
    .take(40)
    .collect::<String>()
    .trim_start_matches('0')
    .to_string();
  CertificateHashData {
    hash_algorithm: "SHA256".to_string(),
    issuer_name_hash,
    issuer_key_hash,
    serial_number: if serial_number.is_empty() {
      "1".to_string()
    } else {
      serial_number
    },
  }
}

fn synthetic_hex(value: &str, seed: u64) -> String {
  let mut hash = seed;
  for byte in value.bytes() {
    hash ^= u64::from(byte);
    hash = hash.wrapping_mul(0x100_0000_01b3);
    hash = hash.rotate_left(5);
  }
  format!(
    "{hash:016x}{:016x}{:016x}{:016x}",
    hash.rotate_left(17),
    hash.rotate_left(31),
    hash.rotate_left(47)
  )
}

fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
  let mut reader = BufReader::new(File::open(path)?);
  let certs = rustls_pemfile::certs(&mut reader)
    .collect::<std::result::Result<Vec<_>, _>>()?;
  if certs.is_empty() {
    return Err(anyhow!("No certificates found in {}.", path.display()));
  }
  Ok(certs)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
  let mut reader = BufReader::new(File::open(path)?);
  rustls_pemfile::private_key(&mut reader)?
    .ok_or_else(|| anyhow!("No private key found in {}.", path.display()))
}

fn is_central_system_root_certificate_type(certificate_type: &str) -> bool {
  CertificateType::parse(certificate_type)
    .is_some_and(CertificateType::is_central_system_root)
}

fn is_simulated_invalid(value: &str) -> bool {
  value.trim().is_empty() || normalize_identifier(value).contains("invalid")
}
