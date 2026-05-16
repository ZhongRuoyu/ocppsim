use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{ArgAction, Parser, ValueEnum};

use crate::config::{ProfileDefaults, resolve_profile};
use crate::ocpp::OcppVersion;

const DEFAULT_CONNECTORS: u16 = 1;
const DEFAULT_PROTOCOL: OcppVersion = OcppVersion::V1_6;
const DEFAULT_VENDOR: &str = "ocppsim";
const DEFAULT_MODEL: &str = "ocppsim";
const DEFAULT_FIRMWARE: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_CONFIG_PATH_HINT: &str = "~/.config/ocppsim/ocppsim.toml";
const CLI_LONG_ABOUT: &str =
  "Command-line OCPP charge point simulator for OCPP-J.
Run in direct mode with --ws-url and --cp-id,
or pass a profile name from a TOML config file.";
const CLI_AFTER_HELP: &str = r#"Config file format:
  # Optional global defaults that can be overridden by charge point configs.
  # CLI options take precedence over config file items.
  protocol = "2.1"
  vendor = "ocppsim"
  model = "ocppsim"
  firmware = "0.1.0"
  log-path = "./ocppsim.log"
  trace-frames = false
  strict = false
  request-timeout-seconds = 30
  heartbeat-seconds = 0

  [charge-points.example]
  ws-url = "wss://csms.example.com/ocpp"
  id = "CP-001"
  append-cp-id = true
  connectors = 1

Examples:
  ocppsim --ws-url ws://csms.local/ocpp --cp-id CP-001
  ocppsim --ws-url ws://csms.local/ocpp --cp-id CP-001 --log-path ./ocpp.log
  ocppsim example
  ocppsim example --config-path ./config.toml
"#;

/// Command-line OCPP charge point simulator for OCPP-J
#[derive(Debug, Clone, Parser)]
#[command(
  name = "ocppsim",
  long_about = CLI_LONG_ABOUT,
  after_long_help = CLI_AFTER_HELP,
  version,
  long_version = crate::version_string(),
)]
pub struct CliArgs {
  /// Profile name from config file
  #[arg(value_name = "PROFILE")]
  pub profile: Option<String>,

  /// Config file path. Defaults to ~/.config/ocppsim/ocppsim.toml
  #[arg(long, value_name = "PATH")]
  pub config_path: Option<PathBuf>,

  /// CSMS WebSocket URL. Required without PROFILE
  #[arg(long, value_name = "URL")]
  pub ws_url: Option<String>,

  /// Charge point ID. Required without PROFILE
  #[arg(long, value_name = "ID")]
  pub cp_id: Option<String>,

  /// Do not append cp-id to ws-url
  #[arg(
    long,
    default_value_t = false,
    action = ArgAction::SetTrue,
  )]
  pub no_append_cp_id: bool,

  /// Connector count (must be positive)
  #[arg(long)]
  pub connectors: Option<u16>,

  /// OCPP protocol version (overrides profile value)
  #[arg(long, value_enum)]
  pub protocol: Option<ProtocolArg>,

  /// Charge point vendor string
  #[arg(long)]
  pub vendor: Option<String>,

  /// Charge point model string
  #[arg(long)]
  pub model: Option<String>,

  /// Firmware version string
  #[arg(long)]
  pub firmware: Option<String>,

  /// Append logs to file at PATH
  #[arg(long, value_name = "PATH")]
  pub log_path: Option<PathBuf>,

  /// Log raw OCPP JSON frames
  #[arg(
    long,
    default_value_t = false,
    action = ArgAction::SetTrue,
  )]
  pub trace_frames: bool,

  /// Validate inbound CALL payloads against checked-in JSON schemas
  #[arg(
    long,
    default_value_t = false,
    action = ArgAction::SetTrue,
  )]
  pub strict: bool,

  /// Request timeout in seconds
  #[arg(long)]
  pub request_timeout_seconds: Option<u64>,

  /// Startup periodic heartbeat interval in seconds (0 disables periodic
  /// heartbeats)
  #[arg(long)]
  pub heartbeat_seconds: Option<u64>,
}

/// Fully resolved runtime arguments after merging CLI and config profile data.
#[derive(Debug, Clone)]
pub struct ResolvedCliArgs {
  pub profile: Option<String>,
  pub config_path: Option<PathBuf>,
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
}

impl CliArgs {
  /// Resolves CLI arguments into effective simulator settings.
  ///
  /// This method merges, in order of precedence:
  /// 1. Built-in defaults.
  /// 2. Global profile defaults from config.
  /// 3. Charge-point profile values.
  /// 4. Explicit CLI overrides.
  ///
  /// Returns an error when required direct-mode flags are missing, when
  /// profile constraints are violated, or when overrides are invalid.
  pub fn resolve(self) -> Result<ResolvedCliArgs> {
    if self.profile.is_some() && (self.ws_url.is_some() || self.cp_id.is_some())
    {
      bail!("When a profile is used, --ws-url and --cp-id must not be set.");
    }

    let mut resolved = if let Some(profile_name) = &self.profile {
      let config_path = self
        .config_path
        .clone()
        .map(|path| expand_tilde_path(&path))
        .unwrap_or_else(default_config_path);
      let defaults = ProfileDefaults {
        connectors: DEFAULT_CONNECTORS,
        protocol: DEFAULT_PROTOCOL,
        vendor: DEFAULT_VENDOR.to_string(),
        model: DEFAULT_MODEL.to_string(),
        firmware: DEFAULT_FIRMWARE.to_string(),
        request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
      };
      let profile = resolve_profile(&config_path, profile_name, &defaults)?;
      ResolvedCliArgs {
        profile: Some(profile_name.clone()),
        config_path: Some(config_path),
        ws_url: profile.ws_url,
        cp_id: profile.cp_id,
        append_cp_id: profile.append_cp_id,
        connectors: profile.connectors,
        protocol: profile.protocol,
        vendor: profile.vendor,
        model: profile.model,
        firmware: profile.firmware,
        log_path: profile.log_path.map(|path| expand_tilde_path(&path)),
        trace_frames: profile.trace_frames,
        strict: profile.strict,
        request_timeout_seconds: profile.request_timeout_seconds,
        heartbeat_seconds: profile.heartbeat_seconds,
      }
    } else {
      resolve_with_direct_args(&self)?
    };

    apply_cli_overrides(&mut resolved, &self)?;
    Ok(resolved)
  }
}

/// CLI-facing protocol selector accepted by clap value parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProtocolArg {
  #[value(name = "1.6")]
  V1_6,
  #[value(name = "2.0.1")]
  V2_0_1,
  #[value(name = "2.1")]
  V2_1,
}

impl ProtocolArg {
  /// Maps the CLI enum variant to the internal OCPP version enum.
  pub fn to_version(self) -> OcppVersion {
    match self {
      Self::V1_6 => OcppVersion::V1_6,
      Self::V2_0_1 => OcppVersion::V2_0_1,
      Self::V2_1 => OcppVersion::V2_1,
    }
  }
}

/// Resolves configuration in direct mode, without profile loading.
///
/// Inputs must include both `--ws-url` and `--cp-id`. The function returns
/// baseline defaults plus direct CLI protocol selection.
fn resolve_with_direct_args(cli: &CliArgs) -> Result<ResolvedCliArgs> {
  let (ws_url, cp_id) = match (&cli.ws_url, &cli.cp_id) {
    (None, None) => {
      bail!("--ws-url and --cp-id are required when no profile is used.");
    }
    (None, Some(_)) => {
      bail!("--ws-url is required when no profile is used.");
    }
    (Some(_), None) => {
      bail!("--cp-id is required when no profile is used.");
    }
    (Some(ws_url), Some(cp_id)) => (ws_url.clone(), cp_id.clone()),
  };

  Ok(ResolvedCliArgs {
    profile: None,
    config_path: None,
    ws_url,
    cp_id,
    append_cp_id: true,
    connectors: DEFAULT_CONNECTORS,
    protocol: cli
      .protocol
      .map(ProtocolArg::to_version)
      .unwrap_or(DEFAULT_PROTOCOL),
    vendor: DEFAULT_VENDOR.to_string(),
    model: DEFAULT_MODEL.to_string(),
    firmware: DEFAULT_FIRMWARE.to_string(),
    log_path: None,
    trace_frames: false,
    strict: false,
    request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
    heartbeat_seconds: None,
  })
}

/// Applies optional CLI overrides on top of already-resolved settings.
///
/// This function mutates `resolved` and validates override-specific
/// constraints, such as positive connector counts.
fn apply_cli_overrides(
  resolved: &mut ResolvedCliArgs,
  cli: &CliArgs,
) -> Result<()> {
  if cli.no_append_cp_id {
    resolved.append_cp_id = false;
  }
  if let Some(connectors) = cli.connectors {
    if connectors == 0 {
      bail!("--connectors must be positive.");
    }
    resolved.connectors = connectors;
  }
  if let Some(protocol) = cli.protocol {
    resolved.protocol = protocol.to_version();
  }
  if let Some(vendor) = &cli.vendor {
    resolved.vendor = vendor.clone();
  }
  if let Some(model) = &cli.model {
    resolved.model = model.clone();
  }
  if let Some(firmware) = &cli.firmware {
    resolved.firmware = firmware.clone();
  }
  if let Some(path) = &cli.log_path {
    resolved.log_path = Some(expand_tilde_path(path));
  }
  if cli.trace_frames {
    resolved.trace_frames = true;
  }
  if cli.strict {
    resolved.strict = true;
  }
  if let Some(seconds) = cli.request_timeout_seconds {
    resolved.request_timeout_seconds = seconds;
  }
  if let Some(seconds) = cli.heartbeat_seconds {
    resolved.heartbeat_seconds = normalize_heartbeat_seconds(Some(seconds));
  }
  Ok(())
}

/// Converts heartbeat seconds into an optional interval.
///
/// A value of `0` disables periodic heartbeats and becomes `None`.
fn normalize_heartbeat_seconds(value: Option<u64>) -> Option<u64> {
  value.and_then(|seconds| if seconds == 0 { None } else { Some(seconds) })
}

/// Expands `~` and `~/...` paths against the current `HOME` directory.
///
/// If no `HOME` is present, or if the path does not start with `~`, the
/// original path is returned unchanged.
fn expand_tilde_path(path: &Path) -> PathBuf {
  let raw = path.to_string_lossy();
  if (raw == "~" || raw.starts_with("~/"))
    && let Some(home) = env::var_os("HOME")
  {
    let mut expanded = PathBuf::from(home);
    if raw.len() > 2 {
      expanded.push(raw.trim_start_matches("~/"));
    }
    return expanded;
  }

  path.to_path_buf()
}

/// Returns the default config file location for profile mode.
///
/// The path is `$HOME/.config/ocppsim/ocppsim.toml` when `HOME` exists,
/// otherwise a relative fallback of `.config/ocppsim/ocppsim.toml`.
fn default_config_path() -> PathBuf {
  if let Some(home) = env::var_os("HOME") {
    let mut path = PathBuf::from(home);
    path.push(DEFAULT_CONFIG_PATH_HINT.trim_start_matches("~/"));
    return path;
  }

  PathBuf::from(DEFAULT_CONFIG_PATH_HINT.trim_start_matches("~/"))
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::{Path, PathBuf};
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::time::{SystemTime, UNIX_EPOCH};

  use super::{CliArgs, OcppVersion, default_config_path, expand_tilde_path};

  static TEMP_CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

  /// Builds baseline profile-mode args used by test cases.
  fn base_profile_args() -> CliArgs {
    CliArgs {
      profile: Some("test".to_string()),
      config_path: None,
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    }
  }

  #[test]
  /// Verifies profile mode rejects simultaneous direct connection flags.
  fn rejects_profile_with_ws_url_or_cp_id() {
    let mut args = base_profile_args();
    args.ws_url = Some("ws://example".to_string());
    assert!(args.resolve().is_err());
  }

  #[test]
  /// Verifies direct mode requires both `--ws-url` and `--cp-id`.
  fn direct_args_require_ws_url_and_cp_id() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: None,
      cp_id: None,
      no_append_cp_id: true,
      connectors: Some(2),
      protocol: Some(super::ProtocolArg::V1_6),
      vendor: Some("vendor".to_string()),
      model: Some("model".to_string()),
      firmware: Some("1.0.0".to_string()),
      log_path: None,
      trace_frames: true,
      strict: false,
      request_timeout_seconds: Some(20),
      heartbeat_seconds: Some(5),
    };
    let error = args.resolve().expect_err("resolution should fail");
    assert_eq!(
      error.to_string(),
      "--ws-url and --cp-id are required when no profile is used."
    );
  }

  #[test]
  /// Verifies direct mode fails when `--cp-id` is set without `--ws-url`.
  fn direct_args_require_ws_url_when_only_cp_id_is_set() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: None,
      cp_id: Some("CP-DEMO".to_string()),
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("resolution should fail");
    assert_eq!(
      error.to_string(),
      "--ws-url is required when no profile is used."
    );
  }

  #[test]
  /// Verifies direct mode fails when `--ws-url` is set without `--cp-id`.
  fn direct_args_require_cp_id_when_only_ws_url_is_set() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("resolution should fail");
    assert_eq!(
      error.to_string(),
      "--cp-id is required when no profile is used."
    );
  }

  #[test]
  /// Verifies protocol CLI enum converts to the expected internal versions.
  fn protocol_enum_maps_to_version() {
    assert_eq!(super::ProtocolArg::V1_6.to_version(), OcppVersion::V1_6);
    assert_eq!(super::ProtocolArg::V2_0_1.to_version(), OcppVersion::V2_0_1);
    assert_eq!(super::ProtocolArg::V2_1.to_version(), OcppVersion::V2_1);
  }

  #[test]
  /// Verifies profile resolution loads required fields from a config file.
  fn resolves_profile_from_config_path() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.cp_id, "CP-DEMO");
    assert_eq!(resolved.ws_url, "wss://example.com/ocpp");
    assert_eq!(resolved.protocol, OcppVersion::V1_6);
    assert_eq!(resolved.connectors, 1);
    assert_eq!(resolved.vendor, "ocppsim");
    assert_eq!(resolved.model, "ocppsim");
    assert!(resolved.append_cp_id);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies profiles must include both `ws-url` and `id`.
  fn profile_requires_ws_url_and_id() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
id = "CP-DEMO"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    assert!(args.resolve().is_err());

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
    let path =
      base.join(format!(".tmp-ocppsim-{pid}-{timestamp}-{sequence}.toml"));
    fs::write(&path, content).expect("write temp config");
    path
  }

  #[test]
  /// Verifies CLI `--log-path` overrides resolved log file destination.
  fn applies_log_path_override() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: Some(PathBuf::from("./sim.log")),
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("arguments should resolve");
    assert_eq!(resolved.log_path, Some(PathBuf::from("./sim.log")));
  }

  #[test]
  /// Verifies profile-level `log-path` is resolved when present.
  fn resolves_profile_log_path() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
log-path = "./profile.log"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.log_path, Some(PathBuf::from("./profile.log")));

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies profile resolution inherits global defaults when omitted.
  fn resolves_global_defaults_for_profile() {
    let path = write_temp_config(
      r#"
protocol = "1.6"
vendor = "global-vendor"
model = "global-model"
firmware = "global-fw"
log-path = "./global.log"
trace-frames = true
strict = true
request-timeout-seconds = 42
heartbeat-seconds = 12

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.protocol, OcppVersion::V1_6);
    assert_eq!(resolved.vendor, "global-vendor");
    assert_eq!(resolved.model, "global-model");
    assert_eq!(resolved.firmware, "global-fw");
    assert_eq!(resolved.log_path, Some(PathBuf::from("./global.log")));
    assert!(resolved.trace_frames);
    assert!(resolved.strict);
    assert_eq!(resolved.request_timeout_seconds, 42);
    assert_eq!(resolved.heartbeat_seconds, Some(12));

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies charge-point config takes precedence over global defaults.
  fn profile_overrides_global_defaults() {
    let path = write_temp_config(
      r#"
protocol = "2.0.1"
vendor = "global-vendor"
model = "global-model"
firmware = "global-fw"
log-path = "./global.log"
trace-frames = true
strict = true
request-timeout-seconds = 60
heartbeat-seconds = 20

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
protocol = "2.1"
vendor = "profile-vendor"
model = "profile-model"
firmware = "profile-fw"
log-path = "./profile.log"
trace-frames = false
strict = false
request-timeout-seconds = 15
heartbeat-seconds = 0
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.protocol, OcppVersion::V2_1);
    assert_eq!(resolved.vendor, "profile-vendor");
    assert_eq!(resolved.model, "profile-model");
    assert_eq!(resolved.firmware, "profile-fw");
    assert_eq!(resolved.log_path, Some(PathBuf::from("./profile.log")));
    assert!(!resolved.trace_frames);
    assert!(!resolved.strict);
    assert_eq!(resolved.request_timeout_seconds, 15);
    assert_eq!(resolved.heartbeat_seconds, None);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies CLI flags override both profile and global config values.
  fn cli_overrides_profile_and_global_values() {
    let path = write_temp_config(
      r#"
protocol = "1.6"
vendor = "global-vendor"
model = "global-model"
firmware = "global-fw"
log-path = "./global.log"
trace-frames = false
request-timeout-seconds = 60
heartbeat-seconds = 20

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
protocol = "2.0.1"
vendor = "profile-vendor"
model = "profile-model"
firmware = "profile-fw"
log-path = "./profile.log"
trace-frames = false
request-timeout-seconds = 15
heartbeat-seconds = 12
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: Some(super::ProtocolArg::V2_1),
      vendor: Some("cli-vendor".to_string()),
      model: Some("cli-model".to_string()),
      firmware: Some("cli-fw".to_string()),
      log_path: Some(PathBuf::from("./cli.log")),
      trace_frames: true,
      strict: true,
      request_timeout_seconds: Some(99),
      heartbeat_seconds: Some(0),
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.protocol, OcppVersion::V2_1);
    assert_eq!(resolved.vendor, "cli-vendor");
    assert_eq!(resolved.model, "cli-model");
    assert_eq!(resolved.firmware, "cli-fw");
    assert_eq!(resolved.log_path, Some(PathBuf::from("./cli.log")));
    assert!(resolved.trace_frames);
    assert!(resolved.strict);
    assert_eq!(resolved.request_timeout_seconds, 99);
    assert_eq!(resolved.heartbeat_seconds, None);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies global heartbeat value `0` disables startup heartbeats.
  fn global_heartbeat_zero_disables_periodic_heartbeat() {
    let path = write_temp_config(
      r#"
heartbeat-seconds = 0

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.heartbeat_seconds, None);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies CLI heartbeat value `0` disables startup heartbeats.
  fn cli_heartbeat_zero_disables_periodic_heartbeat() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: Some(0),
    };

    let resolved = args.resolve().expect("arguments should resolve");
    assert_eq!(resolved.heartbeat_seconds, None);
  }

  #[test]
  /// Verifies tilde expansion for CLI paths and helper behavior.
  fn expands_tilde_in_cli_paths() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: Some(PathBuf::from("~/sim.log")),
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("arguments should resolve");
    assert_eq!(
      resolved.log_path,
      Some(expand_tilde_path(Path::new("~/sim.log")))
    );
    assert_eq!(
      expand_tilde_path(Path::new("~/.config/ocppsim/config.toml")),
      default_config_path().with_file_name("config.toml")
    );
  }

  #[test]
  /// Verifies that a nonexistent config file path produces an error.
  fn missing_config_file_produces_error() {
    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(PathBuf::from("/nonexistent/path/ocppsim.toml")),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("Failed to read"),
      "unexpected error: {error}"
    );
  }

  #[test]
  /// Verifies that referencing a nonexistent profile produces an error.
  fn nonexistent_profile_produces_error() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
"#,
    );

    let args = CliArgs {
      profile: Some("no-such-profile".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("was not found"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies that an invalid protocol string in a profile rejects.
  fn invalid_protocol_in_profile_rejects() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
protocol = "3.0"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("protocol"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies that an invalid protocol in global config rejects.
  fn invalid_global_protocol_rejects() {
    let path = write_temp_config(
      r#"
protocol = "invalid"

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("protocol"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies that `connectors = 0` in a profile is rejected.
  fn profile_connectors_zero_rejects() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
connectors = 0
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("connectors"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies that `--connectors 0` on the CLI is rejected.
  fn cli_connectors_zero_rejects() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      no_append_cp_id: false,
      connectors: Some(0),
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("--connectors must be positive"),
      "unexpected error: {error}"
    );
  }

  #[test]
  /// Verifies that `append-cp-id = false` in profile disables appending.
  fn profile_append_cp_id_false() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
append-cp-id = false
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert!(!resolved.append_cp_id);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies `--no-append-cp-id` overrides profile `append-cp-id = true`.
  fn cli_no_append_cp_id_overrides_profile() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
append-cp-id = true
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: true,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert!(!resolved.append_cp_id);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies direct mode applies expected default values correctly.
  fn direct_mode_applies_defaults() {
    let args = CliArgs {
      profile: None,
      config_path: None,
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("arguments should resolve");
    assert_eq!(resolved.ws_url, "ws://localhost:9000/ocpp");
    assert_eq!(resolved.cp_id, "CP-TEST");
    assert!(resolved.append_cp_id);
    assert_eq!(resolved.connectors, 1);
    assert_eq!(resolved.protocol, OcppVersion::V1_6);
    assert_eq!(resolved.vendor, "ocppsim");
    assert_eq!(resolved.model, "ocppsim");
    assert_eq!(resolved.log_path, None);
    assert!(!resolved.trace_frames);
    assert_eq!(resolved.request_timeout_seconds, 30);
    assert_eq!(resolved.heartbeat_seconds, None);
  }

  #[test]
  /// Verifies that profile connectors setting is respected.
  fn profile_connectors_override() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
connectors = 3
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.connectors, 3);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies CLI `--connectors` overrides profile value.
  fn cli_connectors_overrides_profile() {
    let path = write_temp_config(
      r#"
[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
connectors = 3
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: Some(5),
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.connectors, 5);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies malformed TOML config file produces a parse error.
  fn malformed_toml_produces_error() {
    let path = write_temp_config("this is not valid toml [[[");

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ws_url: None,
      cp_id: None,
      no_append_cp_id: false,
      connectors: None,
      protocol: None,
      vendor: None,
      model: None,
      firmware: None,
      log_path: None,
      trace_frames: false,
      strict: false,
      request_timeout_seconds: None,
      heartbeat_seconds: None,
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("Failed to parse"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies that expand_tilde_path leaves non-tilde paths unchanged.
  fn expand_tilde_path_leaves_absolute_unchanged() {
    let path = Path::new("/usr/local/bin/ocppsim");
    assert_eq!(expand_tilde_path(path), PathBuf::from(path));
  }

  #[test]
  /// Verifies that expand_tilde_path leaves relative paths unchanged.
  fn expand_tilde_path_leaves_relative_unchanged() {
    let path = Path::new("relative/path.toml");
    assert_eq!(expand_tilde_path(path), PathBuf::from(path));
  }
}
