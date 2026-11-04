use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::ocpp::{
  OcppVersion, basic_auth_password_requirement, is_valid_basic_auth_password,
  validate_boot_notification_fields, validate_heartbeat_interval,
};

/// Built-in defaults used when profile/global config entries are omitted.
#[derive(Debug, Clone)]
pub struct ConfigDefaults {
  pub connectors: u16,
  pub protocol: OcppVersion,
  pub vendor: String,
  pub model: String,
  pub firmware: String,
  pub request_timeout_seconds: u64,
  pub outbound_queue_limit: usize,
  pub security_event_limit: usize,
}

/// Final settings after merging defaults, global config, and a profile.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
  pub ws_url: Option<String>,
  pub cp_id: Option<String>,
  pub append_cp_id: bool,
  pub connectors: u16,
  pub protocol: OcppVersion,
  pub vendor: String,
  pub model: String,
  pub firmware: String,
  pub log_path: Option<PathBuf>,
  pub trace_frames: bool,
  pub strict: bool,
  pub request_timeout_seconds: u64,
  pub heartbeat_seconds: Option<u64>,
  pub outbound_queue_limit: usize,
  pub security_event_limit: usize,
  pub security_profile: Option<u8>,
  pub basic_auth_password: Option<String>,
  pub ca_cert_path: Option<PathBuf>,
  pub client_cert_path: Option<PathBuf>,
  pub client_key_path: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
  protocol: Option<String>,
  vendor: Option<String>,
  model: Option<String>,
  firmware: Option<String>,
  #[serde(rename = "log-path")]
  log_path: Option<PathBuf>,
  #[serde(rename = "trace-frames")]
  trace_frames: Option<bool>,
  strict: Option<bool>,
  #[serde(rename = "request-timeout-seconds")]
  request_timeout_seconds: Option<u64>,
  #[serde(rename = "heartbeat-seconds")]
  heartbeat_seconds: Option<u64>,
  #[serde(rename = "outbound-queue-limit")]
  outbound_queue_limit: Option<usize>,
  #[serde(rename = "security-event-limit")]
  security_event_limit: Option<usize>,
  #[serde(rename = "security-profile")]
  security_profile: Option<u8>,
  #[serde(rename = "basic-auth-password")]
  basic_auth_password: Option<String>,
  #[serde(rename = "ca-cert")]
  ca_cert_path: Option<PathBuf>,
  #[serde(rename = "client-cert")]
  client_cert_path: Option<PathBuf>,
  #[serde(rename = "client-key")]
  client_key_path: Option<PathBuf>,
  #[serde(default, rename = "charge-points")]
  charge_points: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct ProfileConfig {
  #[serde(rename = "ws-url")]
  ws_url: Option<String>,
  id: Option<String>,
  #[serde(rename = "append-cp-id")]
  append_cp_id: Option<bool>,
  connectors: Option<u16>,
  protocol: Option<String>,
  vendor: Option<String>,
  model: Option<String>,
  firmware: Option<String>,
  #[serde(rename = "log-path")]
  log_path: Option<PathBuf>,
  #[serde(rename = "trace-frames")]
  trace_frames: Option<bool>,
  strict: Option<bool>,
  #[serde(rename = "request-timeout-seconds")]
  request_timeout_seconds: Option<u64>,
  #[serde(rename = "heartbeat-seconds")]
  heartbeat_seconds: Option<u64>,
  #[serde(rename = "outbound-queue-limit")]
  outbound_queue_limit: Option<usize>,
  #[serde(rename = "security-event-limit")]
  security_event_limit: Option<usize>,
  #[serde(rename = "security-profile")]
  security_profile: Option<u8>,
  #[serde(rename = "basic-auth-password")]
  basic_auth_password: Option<String>,
  #[serde(rename = "ca-cert")]
  ca_cert_path: Option<PathBuf>,
  #[serde(rename = "client-cert")]
  client_cert_path: Option<PathBuf>,
  #[serde(rename = "client-key")]
  client_key_path: Option<PathBuf>,
}

/// Resolves global settings and an optional charge point profile.
///
/// A selected profile must define `ws-url` and `id`. Other values inherit
/// from global config and then from `defaults`. A missing file is allowed
/// only when `allow_missing` is true and no profile is selected.
pub fn resolve_config(
  config_path: &Path,
  profile_name: Option<&str>,
  defaults: &ConfigDefaults,
  allow_missing: bool,
) -> Result<ResolvedConfig> {
  let mut global =
    load_config(config_path, allow_missing && profile_name.is_none())?;
  let profile = select_profile(&mut global, config_path, profile_name)?;
  let source = profile_name.map_or_else(
    || "Global config".to_string(),
    |name| format!("Profile `{name}`"),
  );

  let protocol_label =
    profile.protocol.as_deref().or(global.protocol.as_deref());
  let protocol = if let Some(label) = protocol_label {
    parse_protocol_label(label).with_context(|| {
      format!(
        "Invalid `protocol` in {} ({}).",
        source,
        config_path.display()
      )
    })?
  } else {
    defaults.protocol
  };

  let security_profile = profile.security_profile.or(global.security_profile);
  if let Some(profile_number) = security_profile
    && !(1..=3).contains(&profile_number)
  {
    bail!(
      "{} has invalid `security-profile={}` in {}.",
      source,
      profile_number,
      config_path.display()
    );
  }

  let basic_auth_password =
    profile.basic_auth_password.or(global.basic_auth_password);
  if let Some(password) = basic_auth_password.as_deref()
    && !is_valid_basic_auth_password(protocol, password)
  {
    bail!(
      "{} has invalid `basic-auth-password` in {}. \
      Expected {}.",
      source,
      config_path.display(),
      basic_auth_password_requirement(protocol)
    );
  }

  let vendor = merged_string(profile.vendor, global.vendor, &defaults.vendor);
  let model = merged_string(profile.model, global.model, &defaults.model);
  let firmware =
    merged_string(profile.firmware, global.firmware, &defaults.firmware);
  validate_boot_identity(
    &source,
    config_path,
    protocol,
    &vendor,
    &model,
    &firmware,
  )?;

  Ok(ResolvedConfig {
    ws_url: profile.ws_url,
    cp_id: profile.id,
    append_cp_id: profile.append_cp_id.unwrap_or(true),
    connectors: profile.connectors.unwrap_or(defaults.connectors),
    protocol,
    vendor,
    model,
    firmware,
    log_path: profile.log_path.or(global.log_path),
    trace_frames: profile
      .trace_frames
      .or(global.trace_frames)
      .unwrap_or(false),
    strict: profile.strict.or(global.strict).unwrap_or(false),
    request_timeout_seconds: profile
      .request_timeout_seconds
      .or(global.request_timeout_seconds)
      .unwrap_or(defaults.request_timeout_seconds),
    heartbeat_seconds: normalize_heartbeat_seconds(
      profile.heartbeat_seconds.or(global.heartbeat_seconds),
    )?,
    outbound_queue_limit: profile
      .outbound_queue_limit
      .or(global.outbound_queue_limit)
      .unwrap_or(defaults.outbound_queue_limit),
    security_event_limit: profile
      .security_event_limit
      .or(global.security_event_limit)
      .unwrap_or(defaults.security_event_limit),
    security_profile,
    basic_auth_password,
    ca_cert_path: profile.ca_cert_path.or(global.ca_cert_path),
    client_cert_path: profile.client_cert_path.or(global.client_cert_path),
    client_key_path: profile.client_key_path.or(global.client_key_path),
  })
}

/// Selects and validates profile-specific target fields when requested.
fn select_profile(
  config: &mut ConfigFile,
  config_path: &Path,
  profile_name: Option<&str>,
) -> Result<ProfileConfig> {
  let Some(name) = profile_name else {
    return Ok(ProfileConfig::default());
  };
  let mut profile = config.charge_points.remove(name).ok_or_else(|| {
    anyhow!(
      "Profile `{}` was not found in {}.",
      name,
      config_path.display()
    )
  })?;
  profile.ws_url = Some(required_profile_value(
    profile.ws_url,
    name,
    config_path,
    "ws-url",
  )?);
  profile.id =
    Some(required_profile_value(profile.id, name, config_path, "id")?);
  if let Some(connectors) = profile.connectors {
    validate_profile_connectors(name, config_path, connectors)?;
  }
  Ok(profile)
}

fn merged_string(
  profile: Option<String>,
  global: Option<String>,
  default: &str,
) -> String {
  profile.or(global).unwrap_or_else(|| default.to_string())
}

fn validate_profile_connectors(
  profile_name: &str,
  config_path: &Path,
  connectors: u16,
) -> Result<()> {
  if connectors != 0 {
    return Ok(());
  }
  bail!(
    "Profile `{}` has invalid `connectors=0` in {}.",
    profile_name,
    config_path.display()
  );
}

fn validate_boot_identity(
  source: &str,
  config_path: &Path,
  protocol: OcppVersion,
  vendor: &str,
  model: &str,
  firmware: &str,
) -> Result<()> {
  validate_boot_notification_fields(protocol, vendor, model, firmware).map_err(
    |message| {
      anyhow!(
        "{} has invalid BootNotification identity in {}. {}",
        source,
        config_path.display(),
        message
      )
    },
  )
}

fn required_profile_value(
  value: Option<String>,
  profile_name: &str,
  config_path: &Path,
  key: &str,
) -> Result<String> {
  value.ok_or_else(|| {
    anyhow!(
      "Profile `{}` is missing `{}` in {}.",
      profile_name,
      key,
      config_path.display()
    )
  })
}

/// Returns sorted charge point profile names from a TOML config file.
pub fn profile_names(config_path: &Path) -> Result<Vec<String>> {
  let content = std::fs::read_to_string(config_path)
    .with_context(|| format!("Failed to read {}", config_path.display()))?;
  let config: toml::Value = toml::from_str(&content)
    .with_context(|| format!("Failed to parse {}", config_path.display()))?;

  let Some(charge_points) =
    config.get("charge-points").and_then(toml::Value::as_table)
  else {
    return Ok(Vec::new());
  };

  let mut names = charge_points
    .iter()
    .filter_map(|(name, value)| value.as_table().map(|_| name.clone()))
    .collect::<Vec<_>>();
  names.sort();
  Ok(names)
}

/// Reads configuration, optionally tolerating an absent default file.
fn load_config(path: &Path, allow_missing: bool) -> Result<ConfigFile> {
  let content = match std::fs::read_to_string(path) {
    Ok(content) => content,
    Err(error)
      if allow_missing && error.kind() == std::io::ErrorKind::NotFound =>
    {
      return Ok(ConfigFile::default());
    }
    Err(error) => {
      return Err(error)
        .with_context(|| format!("Failed to read {}", path.display()));
    }
  };
  toml::from_str(&content)
    .with_context(|| format!("Failed to parse {}", path.display()))
}

/// Parses a protocol label from TOML into an internal OCPP version enum.
fn parse_protocol_label(label: &str) -> Result<OcppVersion> {
  match label {
    "1.6" => Ok(OcppVersion::V1_6),
    "2.0.1" => Ok(OcppVersion::V2_0_1),
    "2.1" => Ok(OcppVersion::V2_1),
    _ => bail!("Expected one of: 1.6, 2.0.1, 2.1. Got `{label}`."),
  }
}

/// Validates and normalizes a startup heartbeat interval.
///
/// A value of `0` disables heartbeats and becomes `None`.
pub fn normalize_heartbeat_seconds(value: Option<u64>) -> Result<Option<u64>> {
  let value = value.filter(|&seconds| seconds != 0);
  if let Some(seconds) = value {
    validate_heartbeat_interval(seconds)
      .map_err(|error| anyhow!("heartbeat-seconds: {error}"))?;
  }
  Ok(value)
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::time::{SystemTime, UNIX_EPOCH};

  static TEMP_CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

  #[test]
  /// Verifies profile-name discovery reads and sorts charge point tables.
  fn profile_names_returns_sorted_charge_point_names() {
    let path = write_temp_config(
      r#"
[charge-points.beta]
ws-url = "wss://example.com/ocpp"
id = "CP-BETA"

[charge-points.alpha]
ws-url = "wss://example.com/ocpp"
id = "CP-ALPHA"
"#,
    );

    let names = super::profile_names(&path).expect("profile names");
    assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies configs without charge point tables complete no profiles.
  fn profile_names_returns_empty_without_charge_points() {
    let path = write_temp_config(
      r#"
protocol = "2.1"
vendor = "ocppsim"
"#,
    );

    let names = super::profile_names(&path).expect("profile names");
    assert_eq!(names, [] as [std::string::String; 0]);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies empty config files do not fail deserialization.
  fn resolve_profile_reports_missing_profile_for_empty_config() {
    let path = write_temp_config("");
    let defaults = super::ConfigDefaults {
      connectors: 1,
      protocol: super::OcppVersion::V1_6,
      vendor: "ocppsim".to_string(),
      model: "ocppsim".to_string(),
      firmware: "test".to_string(),
      request_timeout_seconds: 30,
      outbound_queue_limit: 1000,
      security_event_limit: 1000,
    };

    let error = super::resolve_config(&path, Some("test"), &defaults, false)
      .expect_err("empty config should not resolve a profile");
    assert_eq!(
      error.to_string(),
      format!("Profile `test` was not found in {}.", path.display())
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies invalid Basic Auth passwords in profiles are rejected.
  fn resolve_profile_rejects_invalid_basic_auth_password() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
security-profile = 2
basic-auth-password = "not-a-hex-password"
"#,
    );
    let defaults = super::ConfigDefaults {
      connectors: 1,
      protocol: super::OcppVersion::V1_6,
      vendor: "ocppsim".to_string(),
      model: "ocppsim".to_string(),
      firmware: "test".to_string(),
      request_timeout_seconds: 30,
      outbound_queue_limit: 1000,
      security_event_limit: 1000,
    };

    let error = super::resolve_config(&path, Some("demo"), &defaults, false)
      .expect_err("invalid password should not resolve");
    assert!(
      error.to_string().contains("basic-auth-password"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies OCPP 2.x profiles accept passwordString Basic Auth passwords.
  fn resolve_profile_accepts_v2_x_basic_auth_password_string() {
    let path = write_temp_config(
      r#"
protocol = "2.0.1"

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
security-profile = 2
basic-auth-password = "not-a-hex-passwd"
"#,
    );
    let defaults = super::ConfigDefaults {
      connectors: 1,
      protocol: super::OcppVersion::V1_6,
      vendor: "ocppsim".to_string(),
      model: "ocppsim".to_string(),
      firmware: "test".to_string(),
      request_timeout_seconds: 30,
      outbound_queue_limit: 1000,
      security_event_limit: 1000,
    };

    let resolved = super::resolve_config(&path, Some("demo"), &defaults, false)
      .expect("profile");
    assert_eq!(
      resolved.basic_auth_password.as_deref(),
      Some("not-a-hex-passwd")
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies profile `BootNotification` identity strings honor schema limits.
  fn resolve_profile_rejects_schema_invalid_boot_identity() {
    let path = write_temp_config(
      r#"
vendor = "too-long-for-ocpp-one-six"

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
"#,
    );
    let defaults = super::ConfigDefaults {
      connectors: 1,
      protocol: super::OcppVersion::V1_6,
      vendor: "ocppsim".to_string(),
      model: "ocppsim".to_string(),
      firmware: "test".to_string(),
      request_timeout_seconds: 30,
      outbound_queue_limit: 1000,
      security_event_limit: 1000,
    };

    let error = super::resolve_config(&path, Some("demo"), &defaults, false)
      .expect_err("invalid vendor should not resolve");
    assert!(
      error.to_string().contains("BootNotification identity"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  /// Writes temporary TOML config content and returns its path.
  fn write_temp_config(content: &str) -> PathBuf {
    let base = std::env::current_dir().expect("cwd");
    let timestamp = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("time")
      .as_nanos();
    let sequence = TEMP_CONFIG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = base.join(format!(
      ".tmp-ocppsim-config-{pid}-{timestamp}-{sequence}.toml"
    ));
    fs::write(&path, content).expect("write temp config");
    path
  }
}
