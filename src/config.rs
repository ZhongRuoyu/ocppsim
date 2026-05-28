use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::ocpp::{OcppVersion, is_valid_basic_auth_password};

/// Built-in defaults used when profile/global config entries are omitted.
#[derive(Debug, Clone)]
pub struct ProfileDefaults {
  pub connectors: u16,
  pub protocol: OcppVersion,
  pub vendor: String,
  pub model: String,
  pub firmware: String,
  pub request_timeout_seconds: u64,
}

/// Final profile settings after merging defaults, global config, and profile.
#[derive(Debug, Clone)]
pub struct ResolvedProfileConfig {
  pub ws_url: String,
  pub cp_id: String,
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
  pub security_profile: Option<u8>,
  pub basic_auth_password: Option<String>,
  pub ca_cert_path: Option<PathBuf>,
  pub client_cert_path: Option<PathBuf>,
  pub client_key_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Clone)]
struct GlobalConfig {
  protocol: Option<String>,
  vendor: Option<String>,
  model: Option<String>,
  firmware: Option<String>,
  log_path: Option<PathBuf>,
  trace_frames: Option<bool>,
  strict: Option<bool>,
  request_timeout_seconds: Option<u64>,
  heartbeat_seconds: Option<u64>,
  security_profile: Option<u8>,
  basic_auth_password: Option<String>,
  ca_cert_path: Option<PathBuf>,
  client_cert_path: Option<PathBuf>,
  client_key_path: Option<PathBuf>,
}

#[derive(Debug)]
struct ProfileSelection {
  profile: ProfileConfig,
  global: GlobalConfig,
}

/// Resolves one profile from a config file into runtime-ready settings.
///
/// The profile must define `ws-url` and `id`. Other values inherit from
/// global config and then from `defaults` when not present.
pub fn resolve_profile(
  config_path: &Path,
  profile_name: &str,
  defaults: &ProfileDefaults,
) -> Result<ResolvedProfileConfig> {
  let selection = load_profile(config_path, profile_name)?;
  let profile = selection.profile;
  let global = selection.global;

  let ws_url = profile.ws_url.ok_or_else(|| {
    anyhow!(
      "Profile `{}` is missing `ws-url` in {}.",
      profile_name,
      config_path.display()
    )
  })?;
  let cp_id = profile.id.ok_or_else(|| {
    anyhow!(
      "Profile `{}` is missing `id` in {}.",
      profile_name,
      config_path.display()
    )
  })?;

  let connectors = profile.connectors.unwrap_or(defaults.connectors);
  if connectors == 0 {
    bail!(
      "Profile `{}` has invalid `connectors=0` in {}.",
      profile_name,
      config_path.display()
    );
  }

  let protocol_label =
    profile.protocol.as_deref().or(global.protocol.as_deref());
  let protocol = if let Some(label) = protocol_label {
    parse_protocol_label(label).with_context(|| {
      format!(
        "Invalid `protocol` in profile `{}` ({}).",
        profile_name,
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
      "Profile `{}` has invalid `security-profile={}` in {}.",
      profile_name,
      profile_number,
      config_path.display()
    );
  }

  let basic_auth_password =
    profile.basic_auth_password.or(global.basic_auth_password);
  if let Some(password) = basic_auth_password.as_deref()
    && !is_valid_basic_auth_password(password)
  {
    bail!(
      "Profile `{}` has invalid `basic-auth-password` in {}. \
      Expected 32 to 40 ASCII hexadecimal characters.",
      profile_name,
      config_path.display()
    );
  }

  Ok(ResolvedProfileConfig {
    ws_url,
    cp_id,
    append_cp_id: profile.append_cp_id.unwrap_or(true),
    connectors,
    protocol,
    vendor: profile
      .vendor
      .or(global.vendor)
      .unwrap_or_else(|| defaults.vendor.clone()),
    model: profile
      .model
      .or(global.model)
      .unwrap_or_else(|| defaults.model.clone()),
    firmware: profile
      .firmware
      .or(global.firmware)
      .unwrap_or_else(|| defaults.firmware.clone()),
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
    ),
    security_profile,
    basic_auth_password,
    ca_cert_path: profile.ca_cert_path.or(global.ca_cert_path),
    client_cert_path: profile.client_cert_path.or(global.client_cert_path),
    client_key_path: profile.client_key_path.or(global.client_key_path),
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

/// Loads the config file and returns the requested profile plus global values.
fn load_profile(path: &Path, profile_name: &str) -> Result<ProfileSelection> {
  let content = std::fs::read_to_string(path)
    .with_context(|| format!("Failed to read {}", path.display()))?;
  let config: ConfigFile = toml::from_str(&content)
    .with_context(|| format!("Failed to parse {}", path.display()))?;
  let global = GlobalConfig {
    protocol: config.protocol,
    vendor: config.vendor,
    model: config.model,
    firmware: config.firmware,
    log_path: config.log_path,
    trace_frames: config.trace_frames,
    strict: config.strict,
    request_timeout_seconds: config.request_timeout_seconds,
    heartbeat_seconds: config.heartbeat_seconds,
    security_profile: config.security_profile,
    basic_auth_password: config.basic_auth_password,
    ca_cert_path: config.ca_cert_path,
    client_cert_path: config.client_cert_path,
    client_key_path: config.client_key_path,
  };

  config
    .charge_points
    .into_iter()
    .find_map(|(name, profile)| {
      if name == profile_name {
        Some(ProfileSelection {
          profile,
          global: global.clone(),
        })
      } else {
        None
      }
    })
    .ok_or_else(|| {
      anyhow!(
        "Profile `{}` was not found in {}.",
        profile_name,
        path.display()
      )
    })
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

/// Normalizes heartbeat interval semantics for config values.
///
/// A value of `0` disables heartbeats and becomes `None`.
fn normalize_heartbeat_seconds(value: Option<u64>) -> Option<u64> {
  value.and_then(|seconds| if seconds == 0 { None } else { Some(seconds) })
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
    assert!(names.is_empty());

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies empty config files do not fail deserialization.
  fn resolve_profile_reports_missing_profile_for_empty_config() {
    let path = write_temp_config("");
    let defaults = super::ProfileDefaults {
      connectors: 1,
      protocol: super::OcppVersion::V1_6,
      vendor: "ocppsim".to_string(),
      model: "ocppsim".to_string(),
      firmware: "test".to_string(),
      request_timeout_seconds: 30,
    };

    let error = super::resolve_profile(&path, "test", &defaults)
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
    let defaults = super::ProfileDefaults {
      connectors: 1,
      protocol: super::OcppVersion::V1_6,
      vendor: "ocppsim".to_string(),
      model: "ocppsim".to_string(),
      firmware: "test".to_string(),
      request_timeout_seconds: 30,
    };

    let error = super::resolve_profile(&path, "demo", &defaults)
      .expect_err("invalid password should not resolve");
    assert!(
      error.to_string().contains("basic-auth-password"),
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
