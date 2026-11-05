use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::Parser;

use super::*;
use crate::args::{Cli, ResolvedCliArgs};

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct ProfileFile(PathBuf);

impl ProfileFile {
  fn new(text: &str) -> Self {
    let path = std::env::current_dir().unwrap().join(format!(
      ".tmp-ocppsim-profiles-{}-{}.toml",
      std::process::id(),
      FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, text).unwrap();
    Self(path)
  }

  fn resolve(
    &self,
    profile: Option<&str>,
    options: &[&str],
  ) -> anyhow::Result<ResolvedCliArgs> {
    let mut args = vec![
      "ocppsim".to_string(),
      "--config-path".to_string(),
      self.0.display().to_string(),
    ];
    args.extend(profile.map(str::to_string));
    args.extend(options.iter().map(|value| (*value).to_string()));
    Cli::try_parse_from(args)?.args.resolve()
  }
}

impl Drop for ProfileFile {
  fn drop(&mut self) {
    let _ = fs::remove_file(&self.0);
  }
}

fn configured_simulator(
  args: &ResolvedCliArgs,
) -> (Simulator, mpsc::UnboundedReceiver<UiEvent>) {
  let config = SimulatorConfig::from_resolved(args).unwrap();
  let (ui_tx, ui_rx) = mpsc::unbounded_channel();
  let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
  (Simulator::new(config, ui_tx, cmd_tx), ui_rx)
}

#[tokio::test]
async fn toml_configs_initialize_values_origins_and_runtime_state() {
  let file = ProfileFile::new(
    r#"
[configs]
AuthorizeRemoteTxRequests = true
CpoName = "global-cpo"
SupportedFileTransferProtocols = "FTP"
[charge-points.example]
ws-url = "ws://localhost/ocpp"
id = "CP-TEST"
[charge-points.example.configs]
AuthorizeRemoteTxRequests = false
OrganizationName = "example-cpo"
HeartbeatInterval = 12
NumberOfConnectors = 3
SecurityProfile = 0
BasicAuthPassword = "0123456789abcdef0123456789abcdef"
"#,
  );
  let globals = file.resolve(None, &[]).unwrap();
  let (global_simulator, _events) = configured_simulator(&globals);
  assert!(global_simulator.authorize_remote_tx_requests());
  assert_eq!(global_simulator.security.cpo_name, "global-cpo");
  assert_eq!(
    global_simulator.configuration[&ConfigurationKey::CpoName].origin,
    ConfigurationOrigin::Profile
  );
  for protocol in ["1.6", "2.0.1", "2.1"] {
    let resolved = file
      .resolve(Some("example"), &["--protocol", protocol])
      .unwrap();
    let (mut simulator, mut events) = configured_simulator(&resolved);
    crate::simulator::initialize_simulator_runtime(&mut simulator);
    assert!(!simulator.authorize_remote_tx_requests());
    assert_eq!(simulator.connectors.len(), 3);
    assert_eq!(simulator.heartbeat.as_ref().unwrap().seconds, 12);
    assert_eq!(simulator.security.cpo_name, "example-cpo");
    assert!(!simulator.security.cpo_name_is_derived);
    assert!(
      simulator.supports_file_transfer_location("ftp://example.test/log")
    );
    assert!(
      !simulator.supports_file_transfer_location("https://example.test/log")
    );
    for key in [
      ConfigurationKey::CpoName,
      ConfigurationKey::OrganizationName,
      ConfigurationKey::HeartbeatInterval,
      ConfigurationKey::AuthorizeRemoteTxRequests,
      ConfigurationKey::AuthorizationKey,
      ConfigurationKey::BasicAuthPassword,
      ConfigurationKey::NumberOfConnectors,
      ConfigurationKey::SecurityProfile,
    ] {
      assert_eq!(
        simulator.configuration[&key].origin,
        ConfigurationOrigin::Profile
      );
    }
    simulator.show_configuration(None).unwrap();
    let logs = drain_log_messages(&mut events);
    assert!(
      logs
        .iter()
        .all(|line| !line.contains("0123456789abcdef0123456789abcdef"))
    );
    assert!(simulator.queue.is_empty());
    assert!(simulator.security.events.is_empty());
    simulator.stop_heartbeat();
  }
}

#[test]
fn profile_reload_replaces_declared_values_and_clears_old_profile_defaults() {
  let file = ProfileFile::new(
    r#"
[charge-points.a]
ws-url = "ws://localhost/ocpp"
id = "CP-A"
vendor = "vendor-a"
[charge-points.a.configs]
AuthorizeRemoteTxRequests = false
CpoName = "profile-cpo"
SupportedFileTransferProtocols = "FTP"
[charge-points.b]
ws-url = "ws://localhost/ocpp"
id = "CP-B"
vendor = "vendor-b"
"#,
  );
  let a = file.resolve(Some("a"), &[]).unwrap();
  let b = file.resolve(Some("b"), &[]).unwrap();
  let (mut simulator, _events) = configured_simulator(&a);
  simulator.apply_connection_config(
    SimulatorConnectionConfig::from_resolved(&b)
      .unwrap()
      .unwrap(),
  );
  assert!(simulator.authorize_remote_tx_requests());
  assert_eq!(
    simulator.configuration[&ConfigurationKey::AuthorizeRemoteTxRequests]
      .origin,
    ConfigurationOrigin::Default
  );
  assert_eq!(simulator.security.cpo_name, "vendor-b");
  assert!(simulator.security.cpo_name_is_derived);
  assert!(
    simulator.supports_file_transfer_location("https://example.test/log")
  );

  simulator
    .set_local_configuration("AuthorizeRemoteTxRequests", "true")
    .unwrap();
  simulator
    .set_local_configuration("CpoName", "runtime-cpo")
    .unwrap();
  simulator.apply_connection_config(
    SimulatorConnectionConfig::from_resolved(&a)
      .unwrap()
      .unwrap(),
  );
  assert!(!simulator.authorize_remote_tx_requests());
  assert_eq!(simulator.security.cpo_name, "profile-cpo");

  assert_eq!(
    simulator.set_configuration_value(
      ConfigurationKey::AuthorizeRemoteTxRequests,
      "true",
      ConfigurationOrigin::Csms,
    ),
    ResponseStatus::Accepted
  );
  simulator
    .set_local_configuration("CpoName", "runtime-cpo")
    .unwrap();
  simulator.apply_connection_config(
    SimulatorConnectionConfig::from_resolved(&b)
      .unwrap()
      .unwrap(),
  );
  assert!(simulator.authorize_remote_tx_requests());
  assert_eq!(
    simulator.configuration[&ConfigurationKey::AuthorizeRemoteTxRequests]
      .origin,
    ConfigurationOrigin::Csms
  );
  assert_eq!(simulator.security.cpo_name, "runtime-cpo");
  simulator.apply_connection_config(
    SimulatorConnectionConfig::from_resolved(&a)
      .unwrap()
      .unwrap(),
  );
  assert!(!simulator.authorize_remote_tx_requests());
  assert_eq!(
    simulator.configuration[&ConfigurationKey::AuthorizeRemoteTxRequests]
      .origin,
    ConfigurationOrigin::Profile
  );
}

#[tokio::test]
async fn startup_and_interactive_profile_loads_report_conflicts() {
  let file = ProfileFile::new(
    r#"
heartbeat-seconds = 10
[configs]
HeartbeatInterval = 20
[charge-points.example]
ws-url = "invalid URL"
id = "CP-TEST"
heartbeat-seconds = 30
[charge-points.example.configs]
HeartbeatInterval = 40
"#,
  );
  let args = Cli::try_parse_from([
    "ocppsim",
    "--config-path",
    file.0.to_str().unwrap(),
    "--heartbeat-seconds",
    "50",
    "example",
  ])
  .unwrap()
  .args;
  let resolved = args.clone().resolve().unwrap();
  let (mut simulator, mut events) = configured_simulator(&resolved);
  crate::simulator::initialize_simulator_runtime(&mut simulator);
  assert_eq!(simulator.heartbeat.as_ref().unwrap().seconds, 50);
  assert_eq!(conflict_warnings(&mut events).len(), 2);
  let next = args.resolve_profile_for_connect("example").unwrap();
  let config = SimulatorConnectionConfig::from_resolved(&next)
    .unwrap()
    .unwrap();
  let (_cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
  // The invalid URL stops before network I/O, after applying the profile.
  assert!(
    simulator
      .handle_offline_command(
        SimulatorCommand::Connect {
          config: Some(Box::new(config)),
        },
        &mut cmd_rx
      )
      .await
      .is_err()
  );
  let warnings = conflict_warnings(&mut events);
  assert_eq!(warnings.len(), 2);
  assert!(warnings[0].contains("global config"));
  assert!(warnings[1].contains("profile `example`"));
  simulator.stop_heartbeat();
}

fn conflict_warnings(
  events: &mut mpsc::UnboundedReceiver<UiEvent>,
) -> Vec<String> {
  let mut warnings = Vec::new();
  while let Ok(event) = events.try_recv() {
    if let UiEvent::Log {
      level: UiLogLevel::Warn,
      message,
    } = event
      && message.contains("conflicts")
    {
      warnings.push(message);
    }
  }
  warnings
}

#[test]
fn invalid_profile_configs_fail_before_runtime_configuration_is_created() {
  for value in [
    "AdditionalRootCertificateCheck = true",
    "LightIntensity = 10",
    "AuthorizeRemoteTxRequests = 'maybe'",
    "WebSocketPingInterval = -1",
  ] {
    let file = ProfileFile::new(&format!("[configs]\n{value}\n"));
    let resolved = file.resolve(None, &[]).unwrap();
    assert!(SimulatorConfig::from_resolved(&resolved).is_err());
    assert!(SimulatorConnectionConfig::from_resolved(&resolved).is_err());
  }
  let file = ProfileFile::new(
    r#"
protocol = "1.6"
[configs]
BasicAuthPassword = "abcdefghijklmnop!"
"#,
  );
  assert!(file.resolve(None, &[]).is_err());
  let resolved = file.resolve(None, &["--protocol", "2.1"]).unwrap();
  assert!(SimulatorConfig::from_resolved(&resolved).is_ok());
}
