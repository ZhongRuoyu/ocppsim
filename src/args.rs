use std::env;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::env::Shells;
use clap_complete::{ArgValueCompleter, CompleteEnv, CompletionCandidate};

use crate::config::{ProfileDefaults, profile_names, resolve_profile};
use crate::ocpp::{
  OcppVersion, basic_auth_password_requirement, is_valid_basic_auth_password,
  validate_boot_notification_fields,
};

const DEFAULT_CONNECTORS: u16 = 1;
const DEFAULT_PROTOCOL: OcppVersion = OcppVersion::V1_6;
const DEFAULT_VENDOR: &str = "ocppsim";
const DEFAULT_MODEL: &str = "ocppsim";
const DEFAULT_FIRMWARE: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_OUTBOUND_QUEUE_LIMIT: usize = 1_000;
const DEFAULT_SECURITY_EVENT_LIMIT: usize = 1_000;
const DEFAULT_CONFIG_PATH_HINT: &str = "~/.config/ocppsim/ocppsim.toml";
const OCPP_2_X_CP_ID_MAX_CHARS: usize = 48;
const CLI_LONG_ABOUT: &str =
  "Command-line OCPP charge point simulator for OCPP-J.
Run without a remote target for local simulation,
with --ws-url and --cp-id for direct mode,
or with a profile name from a TOML config file.";
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
  outbound-queue-limit = 1000
  security-event-limit = 1000
  security-profile = 2
  basic-auth-password = "0123456789abcdef0123456789abcdef"
  ca-cert = "./csms-root.pem"
  client-cert = "./charge-point.pem"
  client-key = "./charge-point-key.pem"

  [charge-points.example]
  ws-url = "wss://csms.example.com/ocpp"
  id = "CP-001"
  append-cp-id = true
  connectors = 1

Examples:
  ocppsim
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
  args_conflicts_with_subcommands = true,
)]
pub struct Cli {
  #[command(subcommand)]
  pub command: Option<CliCommand>,

  #[command(flatten)]
  pub args: CliArgs,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CliCommand {
  /// Output shell completion script
  Completions {
    /// Shell to generate completions for
    #[arg(value_enum)]
    shell: CompletionShell,
  },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
  Bash,
  Elvish,
  Fish,
  Powershell,
  Zsh,
}

impl CompletionShell {
  fn name(self) -> &'static str {
    match self {
      Self::Bash => "bash",
      Self::Elvish => "elvish",
      Self::Fish => "fish",
      Self::Powershell => "powershell",
      Self::Zsh => "zsh",
    }
  }
}

#[derive(Debug, Clone, Args)]
pub struct CliArgs {
  /// Profile name from config file
  #[arg(
    value_name = "PROFILE",
    add = ArgValueCompleter::new(profile_name_completer),
  )]
  pub profile: Option<String>,

  /// Config file path. Defaults to ~/.config/ocppsim/ocppsim.toml
  #[arg(long, value_name = "PATH")]
  pub config_path: Option<PathBuf>,

  /// CSMS WebSocket URL for the initial connection target
  #[arg(long, value_name = "URL")]
  pub ws_url: Option<String>,

  /// Charge point ID for the initial connection target
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

  /// Maximum queued outbound OCPP CALL messages (0 disables the limit)
  #[arg(long, value_name = "COUNT")]
  pub outbound_queue_limit: Option<usize>,

  /// Maximum retained security events (0 disables the limit)
  #[arg(long, value_name = "COUNT")]
  pub security_event_limit: Option<usize>,

  /// OCPP security profile:
  /// 1 = Basic Auth over ws,
  /// 2 = Basic Auth over wss,
  /// 3 = wss with client certificate
  #[arg(long)]
  pub security_profile: Option<u8>,

  /// HTTP Basic password for security profiles 1 and 2
  #[arg(long)]
  pub basic_auth_password: Option<String>,

  /// PEM CA certificate used to trust the CSMS TLS certificate
  #[arg(long, value_name = "PATH")]
  pub ca_cert: Option<PathBuf>,

  /// PEM client certificate for security profile 3
  #[arg(long, value_name = "PATH")]
  pub client_cert: Option<PathBuf>,

  /// PEM private key for security profile 3
  #[arg(long, value_name = "PATH")]
  pub client_key: Option<PathBuf>,
}

/// Handles dynamic completion requests emitted by generated shell scripts.
pub fn complete_from_env() {
  CompleteEnv::with_factory(Cli::command)
    .bin("ocppsim")
    .complete();
}

/// Writes a dynamic shell completion registration script.
pub fn write_completion_script(
  shell: CompletionShell,
  output: &mut dyn Write,
) -> io::Result<()> {
  let command = Cli::command();
  let name = command.get_name().to_string();
  let shells = Shells::builtins();
  let env_shell = shells
    .completer(shell.name())
    .ok_or_else(|| io::Error::other("unsupported completion shell"))?;

  env_shell.write_registration("COMPLETE", &name, &name, &name, output)
}

/// Fully resolved runtime arguments after merging CLI and config profile data.
#[derive(Debug, Clone)]
pub struct ResolvedCliArgs {
  pub profile: Option<String>,
  pub config_path: Option<PathBuf>,
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

impl CliArgs {
  /// Resolves CLI arguments into effective simulator settings.
  ///
  /// This method merges, in order of precedence:
  /// 1. Built-in defaults.
  /// 2. Global profile defaults from config.
  /// 3. Charge-point profile values.
  /// 4. Explicit CLI overrides.
  ///
  /// Returns an error when partial direct-mode flags are provided, when profile
  /// constraints are violated, or when overrides are invalid.
  pub fn resolve(self) -> Result<ResolvedCliArgs> {
    if self.profile.is_some() && (self.ws_url.is_some() || self.cp_id.is_some())
    {
      bail!("When a profile is used, --ws-url and --cp-id must not be set.");
    }

    let mut resolved = if let Some(profile_name) = &self.profile {
      let config_path = self
        .config_path
        .clone()
        .map_or_else(default_config_path, |path| expand_tilde_path(&path));
      let defaults = ProfileDefaults {
        connectors: DEFAULT_CONNECTORS,
        protocol: DEFAULT_PROTOCOL,
        vendor: DEFAULT_VENDOR.to_string(),
        model: DEFAULT_MODEL.to_string(),
        firmware: DEFAULT_FIRMWARE.to_string(),
        request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
        outbound_queue_limit: DEFAULT_OUTBOUND_QUEUE_LIMIT,
        security_event_limit: DEFAULT_SECURITY_EVENT_LIMIT,
      };
      let profile = resolve_profile(&config_path, profile_name, &defaults)?;
      ResolvedCliArgs {
        profile: Some(profile_name.clone()),
        config_path: Some(config_path),
        ws_url: Some(profile.ws_url),
        cp_id: Some(profile.cp_id),
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
        outbound_queue_limit: profile.outbound_queue_limit,
        security_event_limit: profile.security_event_limit,
        security_profile: profile.security_profile,
        basic_auth_password: profile.basic_auth_password,
        ca_cert_path: profile.ca_cert_path.map(|path| expand_tilde_path(&path)),
        client_cert_path: profile
          .client_cert_path
          .map(|path| expand_tilde_path(&path)),
        client_key_path: profile
          .client_key_path
          .map(|path| expand_tilde_path(&path)),
      }
    } else {
      resolve_with_direct_args(&self)?
    };

    apply_cli_overrides(&mut resolved, &self)?;
    validate_basic_auth_password(
      resolved.protocol,
      resolved.basic_auth_password.as_deref(),
      "basic auth password",
    )?;
    validate_charge_point_identity(
      resolved.protocol,
      resolved.cp_id.as_deref(),
      resolved.security_profile,
    )?;
    validate_boot_notification_fields(
      resolved.protocol,
      &resolved.vendor,
      &resolved.model,
      &resolved.firmware,
    )
    .map_err(anyhow::Error::msg)?;
    Ok(resolved)
  }

  /// Resolves a profile supplied to the interactive `connect` command.
  pub fn resolve_profile_for_connect(
    &self,
    profile_name: &str,
  ) -> Result<ResolvedCliArgs> {
    let mut args = self.clone();
    args.profile = Some(profile_name.to_string());
    args.ws_url = None;
    args.cp_id = None;
    args.resolve()
  }

  /// Resolves direct target arguments supplied to the interactive `connect`
  /// command.
  pub fn resolve_direct_for_connect(
    &self,
    ws_url: String,
    cp_id: String,
  ) -> Result<ResolvedCliArgs> {
    let mut args = self.clone();
    args.profile = None;
    args.ws_url = Some(ws_url);
    args.cp_id = Some(cp_id);
    args.resolve()
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

/// Resolves configuration without profile loading.
///
/// Inputs may omit both `--ws-url` and `--cp-id` to start in offline-only
/// local simulation mode. If either direct connection flag is present, both
/// must be present.
fn resolve_with_direct_args(cli: &CliArgs) -> Result<ResolvedCliArgs> {
  let (ws_url, cp_id) = match (&cli.ws_url, &cli.cp_id) {
    (None, None) => (None, None),
    (None, Some(_)) => {
      bail!("--ws-url is required when no profile is used.");
    }
    (Some(_), None) => {
      bail!("--cp-id is required when no profile is used.");
    }
    (Some(ws_url), Some(cp_id)) => (Some(ws_url.clone()), Some(cp_id.clone())),
  };
  let config_path =
    cli.config_path.as_ref().map(|path| expand_tilde_path(path));

  Ok(ResolvedCliArgs {
    profile: None,
    config_path,
    ws_url,
    cp_id,
    append_cp_id: true,
    connectors: DEFAULT_CONNECTORS,
    protocol: cli
      .protocol
      .map_or(DEFAULT_PROTOCOL, ProtocolArg::to_version),
    vendor: DEFAULT_VENDOR.to_string(),
    model: DEFAULT_MODEL.to_string(),
    firmware: DEFAULT_FIRMWARE.to_string(),
    log_path: None,
    trace_frames: false,
    strict: false,
    request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
    heartbeat_seconds: None,
    outbound_queue_limit: DEFAULT_OUTBOUND_QUEUE_LIMIT,
    security_event_limit: DEFAULT_SECURITY_EVENT_LIMIT,
    security_profile: None,
    basic_auth_password: None,
    ca_cert_path: None,
    client_cert_path: None,
    client_key_path: None,
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
    resolved.vendor.clone_from(vendor);
  }
  if let Some(model) = &cli.model {
    resolved.model.clone_from(model);
  }
  if let Some(firmware) = &cli.firmware {
    resolved.firmware.clone_from(firmware);
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
  if let Some(limit) = cli.outbound_queue_limit {
    resolved.outbound_queue_limit = limit;
  }
  if let Some(limit) = cli.security_event_limit {
    resolved.security_event_limit = limit;
  }
  if let Some(profile) = cli.security_profile {
    validate_security_profile(profile)?;
    resolved.security_profile = Some(profile);
  }
  if let Some(password) = &cli.basic_auth_password {
    validate_basic_auth_password(
      resolved.protocol,
      Some(password),
      "--basic-auth-password",
    )?;
    resolved.basic_auth_password = Some(password.clone());
  }
  if let Some(path) = &cli.ca_cert {
    resolved.ca_cert_path = Some(expand_tilde_path(path));
  }
  if let Some(path) = &cli.client_cert {
    resolved.client_cert_path = Some(expand_tilde_path(path));
  }
  if let Some(path) = &cli.client_key {
    resolved.client_key_path = Some(expand_tilde_path(path));
  }
  Ok(())
}

fn validate_security_profile(profile: u8) -> Result<()> {
  if (1..=3).contains(&profile) {
    Ok(())
  } else {
    bail!("security profile must be 1, 2, or 3.")
  }
}

fn validate_basic_auth_password(
  protocol: OcppVersion,
  password: Option<&str>,
  source: &str,
) -> Result<()> {
  if password.is_none_or(|value| is_valid_basic_auth_password(protocol, value))
  {
    Ok(())
  } else {
    bail!(
      "{source} must be {}.",
      basic_auth_password_requirement(protocol)
    )
  }
}

fn validate_charge_point_identity(
  protocol: OcppVersion,
  cp_id: Option<&str>,
  security_profile: Option<u8>,
) -> Result<()> {
  let Some(cp_id) = cp_id else {
    return Ok(());
  };
  if cp_id.is_empty() {
    bail!("charge point ID must not be empty.");
  }

  let is_v2_x = matches!(protocol, OcppVersion::V2_0_1 | OcppVersion::V2_1);
  let uses_basic_auth = matches!(security_profile, Some(1 | 2));
  if cp_id.contains(':') && (is_v2_x || uses_basic_auth) {
    bail!(
      "charge point ID must not contain `:` for OCPP 2.x or Basic Auth \
      profiles."
    );
  }
  if is_v2_x && cp_id.chars().count() > OCPP_2_X_CP_ID_MAX_CHARS {
    bail!(
      "charge point ID must be at most {OCPP_2_X_CP_ID_MAX_CHARS} \
      characters for OCPP {}.",
      protocol.label()
    );
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

/// Completes profile names from the default config location.
fn profile_name_completer(current: &OsStr) -> Vec<CompletionCandidate> {
  profile_name_candidates(current, &default_config_path())
}

fn profile_name_candidates(
  current: &OsStr,
  config_path: &Path,
) -> Vec<CompletionCandidate> {
  let Some(current) = current.to_str() else {
    return Vec::new();
  };
  let Ok(names) = profile_names(config_path) else {
    return Vec::new();
  };

  names
    .into_iter()
    .filter(|name| name.starts_with(current))
    .map(CompletionCandidate::new)
    .collect()
}

/// Returns profile names for interactive command completion.
pub fn profile_completion_names(config_path: Option<&Path>) -> Vec<String> {
  let path = config_path.map_or_else(default_config_path, expand_tilde_path);
  profile_names(&path).unwrap_or_default()
}

#[cfg(test)]
mod tests {
  use std::ffi::OsStr;
  use std::fs;
  use std::path::{Path, PathBuf};
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::time::{SystemTime, UNIX_EPOCH};

  use clap::Parser;

  use super::{
    Cli, CliArgs, CliCommand, CompletionShell, OcppVersion,
    default_config_path, expand_tilde_path, profile_name_candidates,
    write_completion_script,
  };

  static TEMP_CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

  /// Builds all-default args used by test cases.
  fn base_args() -> CliArgs {
    CliArgs {
      profile: None,
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
      outbound_queue_limit: None,
      security_event_limit: None,
      security_profile: None,
      basic_auth_password: None,
      ca_cert: None,
      client_cert: None,
      client_key: None,
    }
  }

  #[test]
  /// Verifies profile mode rejects simultaneous direct connection flags.
  fn rejects_profile_with_ws_url_or_cp_id() {
    let args = CliArgs {
      profile: Some("test".to_string()),
      ws_url: Some("ws://example".to_string()),
      ..base_args()
    };
    assert!(args.resolve().is_err());
  }

  #[test]
  /// Verifies startup can resolve without an initial connection target.
  fn resolves_without_initial_connection_target() {
    let args = CliArgs {
      no_append_cp_id: true,
      connectors: Some(2),
      protocol: Some(super::ProtocolArg::V1_6),
      vendor: Some("vendor".to_string()),
      model: Some("model".to_string()),
      firmware: Some("1.0.0".to_string()),
      trace_frames: true,
      request_timeout_seconds: Some(20),
      heartbeat_seconds: Some(5),
      outbound_queue_limit: Some(12),
      security_event_limit: Some(34),
      ..base_args()
    };
    let resolved = args.resolve().expect("resolution should succeed");
    assert!(resolved.ws_url.is_none());
    assert!(resolved.cp_id.is_none());
    assert_eq!(resolved.config_path, None);
    assert_eq!(resolved.connectors, 2);
    assert_eq!(resolved.vendor, "vendor");
    assert!(resolved.trace_frames);
    assert_eq!(resolved.request_timeout_seconds, 20);
    assert_eq!(resolved.heartbeat_seconds, Some(5));
    assert_eq!(resolved.outbound_queue_limit, 12);
    assert_eq!(resolved.security_event_limit, 34);
  }

  #[test]
  /// Verifies outbound `BootNotification` identity limits are enforced early.
  fn rejects_schema_invalid_boot_notification_identity() {
    let v1_6_args = CliArgs {
      vendor: Some("V".repeat(21)),
      ..base_args()
    };
    let error = v1_6_args
      .resolve()
      .expect_err("should reject vendor length");
    assert!(
      error.to_string().contains("chargePointVendor"),
      "unexpected error: {error}"
    );

    let v2_x_args = CliArgs {
      protocol: Some(super::ProtocolArg::V2_0_1),
      model: Some("M".repeat(21)),
      ..base_args()
    };
    let error = v2_x_args.resolve().expect_err("should reject model length");
    assert!(
      error.to_string().contains("chargingStation.model"),
      "unexpected error: {error}"
    );
  }

  #[test]
  /// Verifies direct mode fails when `--cp-id` is set without `--ws-url`.
  fn direct_args_require_ws_url_when_only_cp_id_is_set() {
    let args = CliArgs {
      cp_id: Some("CP-DEMO".to_string()),
      ..base_args()
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
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      ..base_args()
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
      ..base_args()
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.cp_id.as_deref(), Some("CP-DEMO"));
    assert_eq!(resolved.ws_url.as_deref(), Some("wss://example.com/ocpp"));
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
      ..base_args()
    };
    assert!(args.resolve().is_err());

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies the `completions` subcommand parses before profile mode.
  fn parses_completion_subcommand() {
    let cli = Cli::try_parse_from(["ocppsim", "completions", "bash"])
      .expect("completion command should parse");
    assert!(matches!(
      cli.command,
      Some(CliCommand::Completions {
        shell: CompletionShell::Bash,
      })
    ));
  }

  #[test]
  /// Verifies generated Bash registration delegates back to ocppsim.
  fn bash_completion_script_uses_dynamic_completion() {
    let mut output = Vec::new();
    write_completion_script(CompletionShell::Bash, &mut output)
      .expect("completion script");
    let script = String::from_utf8(output).expect("utf8 script");

    assert!(script.contains("COMPLETE=\"bash\""));
    assert!(script.contains("\"ocppsim\" --"));
  }

  #[test]
  /// Verifies profile completion candidates come from TOML profile names.
  fn profile_name_completion_reads_config_names() {
    let path = write_temp_config(
      r#"
[charge-points.alpha]
ws-url = "wss://example.com/ocpp"
id = "CP-ALPHA"

[charge-points.beta]
ws-url = "wss://example.com/ocpp"
id = "CP-BETA"
"#,
    );

    let candidates = profile_name_candidates(OsStr::new("a"), &path)
      .into_iter()
      .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
      .collect::<Vec<_>>();
    assert_eq!(candidates, vec!["alpha".to_string()]);

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
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      log_path: Some(PathBuf::from("./sim.log")),
      ..base_args()
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
      ..base_args()
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
outbound-queue-limit = 123
security-event-limit = 456

[charge-points.demo]
ws-url = "wss://example.com/ocpp"
id = "CP-DEMO"
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ..base_args()
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
    assert_eq!(resolved.outbound_queue_limit, 123);
    assert_eq!(resolved.security_event_limit, 456);

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
outbound-queue-limit = 123
security-event-limit = 456

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
outbound-queue-limit = 12
security-event-limit = 34
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      ..base_args()
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
    assert_eq!(resolved.outbound_queue_limit, 12);
    assert_eq!(resolved.security_event_limit, 34);

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
outbound-queue-limit = 123
security-event-limit = 456

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
outbound-queue-limit = 12
security-event-limit = 34
"#,
    );

    let args = CliArgs {
      profile: Some("demo".to_string()),
      config_path: Some(path.clone()),
      protocol: Some(super::ProtocolArg::V2_1),
      vendor: Some("cli-vendor".to_string()),
      model: Some("cli-model".to_string()),
      firmware: Some("cli-fw".to_string()),
      log_path: Some(PathBuf::from("./cli.log")),
      trace_frames: true,
      strict: true,
      request_timeout_seconds: Some(99),
      heartbeat_seconds: Some(0),
      outbound_queue_limit: Some(10),
      security_event_limit: Some(20),
      ..base_args()
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
    assert_eq!(resolved.outbound_queue_limit, 10);
    assert_eq!(resolved.security_event_limit, 20);

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
      ..base_args()
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert_eq!(resolved.heartbeat_seconds, None);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies CLI heartbeat value `0` disables startup heartbeats.
  fn cli_heartbeat_zero_disables_periodic_heartbeat() {
    let args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      heartbeat_seconds: Some(0),
      ..base_args()
    };

    let resolved = args.resolve().expect("arguments should resolve");
    assert_eq!(resolved.heartbeat_seconds, None);
  }

  #[test]
  /// Verifies invalid CLI Basic Auth passwords fail before runtime.
  fn cli_basic_auth_password_rejects_invalid_format() {
    let args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      security_profile: Some(1),
      basic_auth_password: Some("not-a-hex-password".to_string()),
      ..base_args()
    };
    let error = args.resolve().expect_err("should reject password");
    assert!(
      error.to_string().contains("ASCII hexadecimal"),
      "unexpected error: {error}"
    );
  }

  #[test]
  /// Verifies OCPP 2.x CLI Basic Auth passwords use passwordString rules.
  fn cli_basic_auth_password_accepts_v2_x_password_string() {
    let args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      protocol: Some(super::ProtocolArg::V2_0_1),
      security_profile: Some(1),
      basic_auth_password: Some("not-a-hex-passwd".to_string()),
      ..base_args()
    };

    let resolved = args.resolve().expect("arguments should resolve");
    assert_eq!(
      resolved.basic_auth_password.as_deref(),
      Some("not-a-hex-passwd")
    );
  }

  #[test]
  /// Verifies Basic Auth profiles reject ambiguous identity separators.
  fn basic_auth_rejects_colon_in_charge_point_id() {
    let args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP:TEST".to_string()),
      security_profile: Some(1),
      basic_auth_password: Some("0123456789abcdef0123456789abcdef".to_string()),
      ..base_args()
    };

    let error = args.resolve().expect_err("should reject colon");
    assert!(
      error.to_string().contains("must not contain `:`"),
      "unexpected error: {error}"
    );
  }

  #[test]
  /// Verifies OCPP 2.x charge point identity limits are enforced.
  fn ocpp_2_x_rejects_invalid_charge_point_id() {
    let colon_args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP:TEST".to_string()),
      protocol: Some(super::ProtocolArg::V2_1),
      ..base_args()
    };
    let error = colon_args.resolve().expect_err("should reject colon");
    assert!(
      error.to_string().contains("must not contain `:`"),
      "unexpected error: {error}"
    );

    let long_args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("C".repeat(49)),
      protocol: Some(super::ProtocolArg::V2_0_1),
      ..base_args()
    };
    let error = long_args.resolve().expect_err("should reject length");
    assert!(
      error.to_string().contains("at most 48"),
      "unexpected error: {error}"
    );
  }

  #[test]
  /// Verifies tilde expansion for CLI paths and helper behavior.
  fn expands_tilde_in_cli_paths() {
    let args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      log_path: Some(PathBuf::from("~/sim.log")),
      ..base_args()
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
      ..base_args()
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
      ..base_args()
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
      ..base_args()
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
      ..base_args()
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
      ..base_args()
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
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      connectors: Some(0),
      ..base_args()
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
      ..base_args()
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
      no_append_cp_id: true,
      ..base_args()
    };

    let resolved = args.resolve().expect("profile should resolve");
    assert!(!resolved.append_cp_id);

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies direct mode applies expected default values correctly.
  fn direct_mode_applies_defaults() {
    let args = CliArgs {
      ws_url: Some("ws://localhost:9000/ocpp".to_string()),
      cp_id: Some("CP-TEST".to_string()),
      ..base_args()
    };

    let resolved = args.resolve().expect("arguments should resolve");
    assert_eq!(resolved.ws_url.as_deref(), Some("ws://localhost:9000/ocpp"));
    assert_eq!(resolved.cp_id.as_deref(), Some("CP-TEST"));
    assert!(resolved.append_cp_id);
    assert_eq!(resolved.connectors, 1);
    assert_eq!(resolved.protocol, OcppVersion::V1_6);
    assert_eq!(resolved.vendor, "ocppsim");
    assert_eq!(resolved.model, "ocppsim");
    assert_eq!(resolved.log_path, None);
    assert!(!resolved.trace_frames);
    assert_eq!(resolved.request_timeout_seconds, 30);
    assert_eq!(resolved.heartbeat_seconds, None);
    assert_eq!(
      resolved.outbound_queue_limit,
      super::DEFAULT_OUTBOUND_QUEUE_LIMIT
    );
    assert_eq!(
      resolved.security_event_limit,
      super::DEFAULT_SECURITY_EVENT_LIMIT
    );
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
      ..base_args()
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
      connectors: Some(5),
      ..base_args()
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
      ..base_args()
    };
    let error = args.resolve().expect_err("should fail");
    assert!(
      error.to_string().contains("Failed to parse"),
      "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
  }

  #[test]
  /// Verifies that [`expand_tilde_path`] leaves non-tilde paths unchanged.
  fn expand_tilde_path_leaves_absolute_unchanged() {
    let path = Path::new("/usr/local/bin/ocppsim");
    assert_eq!(expand_tilde_path(path), PathBuf::from(path));
  }

  #[test]
  /// Verifies that [`expand_tilde_path`] leaves relative paths unchanged.
  fn expand_tilde_path_leaves_relative_unchanged() {
    let path = Path::new("relative/path.toml");
    assert_eq!(expand_tilde_path(path), PathBuf::from(path));
  }
}
