#![doc = include_str!("../README.md")]

mod app;
mod args;
mod commands;
mod config;
mod embedded_schemas;
mod metering;
mod ocpp;
mod sensitive;
mod simulator;
mod version;

use std::backtrace::Backtrace;
use std::panic;
use std::process;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use app::{InputAction, TerminalApp, TerminalSession, restore_console};
use args::{Cli, CliArgs, CliCommand, ResolvedCliArgs};
use clap::Parser;
use commands::{
  ConnectTarget, UserCommand, help_text, parse_command,
  redact_sensitive_command, standards_text,
};
use metering::MeterUpdate;
use ocpp::Measurand;
use sensitive::redact_text_secrets;
use simulator::{
  SimulatorCommand, SimulatorConfig, SimulatorConnectionConfig, run_simulator,
};
use tokio::sync::mpsc;
use version::version_string;

/// Runs the interactive OCPP simulator command-line application.
///
/// # Errors
///
/// Returns an error if the terminal cannot be initialized, the WebSocket
/// connection fails, or a critical runtime error occurs.
pub async fn run() -> Result<()> {
  install_panic_hook();
  args::complete_from_env();
  install_rustls_provider();

  let cli = Cli::parse();
  if let Some(CliCommand::Completions { shell }) = cli.command {
    args::write_completion_script(shell, &mut std::io::stdout())?;
    return Ok(());
  }

  let cli_args = cli.args;
  let resolved = cli_args.clone().resolve()?;
  let protocol = resolved.protocol;

  let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<SimulatorCommand>();
  let (ui_tx, mut ui_rx) = mpsc::unbounded_channel();

  let config = SimulatorConfig::from_resolved(&resolved)?;
  let simulator_task =
    tokio::spawn(run_simulator(config, cmd_rx, ui_tx, cmd_tx.clone()));

  let mut terminal = TerminalSession::new()?;
  let mut app = TerminalApp::new(protocol);
  app.set_profile_completions(args::profile_completion_names(
    resolved.config_path.as_deref(),
  ));
  // Seed taskbar metadata before the simulator emits its first snapshot.
  app.set_connection_target(resolved.profile.clone(), resolved.ws_url.clone());
  if let Some(path) = resolved.log_path.as_ref() {
    app.enable_log_path(path)?;
    app.push_info(format!("Appending logs to {}", path.display()));
  }

  log_profile_source(&mut app, &resolved);
  app.push_info(format!(
    "Simulator ready: cp-id={} protocol={} connectors={}",
    display_optional(resolved.cp_id.as_deref()),
    protocol.label(),
    resolved.connectors
  ));
  app.push_info("Type `help` for commands.");
  app.push_info(
    "Type `connect [<profile> | <ws-url> <cp-id>]` \
    to open a CSMS connection.",
  );

  let mut should_exit = false;
  while !should_exit {
    while let Ok(event) = ui_rx.try_recv() {
      app.apply(event);
    }

    terminal.draw(&mut app)?;
    match TerminalSession::poll_input(&mut app)? {
      InputAction::None => {}
      InputAction::ExitRequested => {
        should_exit = app.request_exit();
      }
      InputAction::Submitted(line) => {
        if line.is_empty() {
          continue;
        }
        app.push_user_input(&redact_sensitive_command(&line));

        match parse_command(&line) {
          Ok(command) => {
            should_exit =
              handle_user_command(command, &cli_args, &cmd_tx, &mut app);
          }
          Err(message) => {
            app.push_error(message);
          }
        }
      }
    }
  }

  terminal.draw(&mut app)?;
  let _ = cmd_tx.send(SimulatorCommand::Shutdown);
  finish_simulator_shutdown(simulator_task).await;
  Ok(())
}

/// Gives graceful shutdown one second before canceling a blocked simulator.
async fn finish_simulator_shutdown(mut task: tokio::task::JoinHandle<()>) {
  if tokio::time::timeout(Duration::from_secs(1), &mut task)
    .await
    .is_err()
  {
    // A peer can stop reading during any write, including the close frame.
    // Dropping the task also drops the connection and its heartbeat ticker.
    task.abort();
    let _ = task.await;
  }
}

/// Installs the `ring` rustls crypto provider for TLS WebSocket connections.
fn install_rustls_provider() {
  let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Installs a process-wide panic hook that restores the terminal and prints
/// a structured panic report with a captured backtrace.
fn install_panic_hook() {
  panic::set_hook(Box::new(|info| {
    restore_console();

    let current_thread = thread::current();
    let thread_name = current_thread.name().unwrap_or("<unnamed>").to_string();
    let location = info.location().map_or_else(
      || "<unknown>".to_string(),
      |loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
    );
    let message = redact_text_secrets(&panic_message(info));
    let backtrace =
      redact_text_secrets(&Backtrace::force_capture().to_string());

    eprintln!();
    eprintln!("==================================================");
    eprintln!("ocppsim panic");
    eprintln!("thread:   {thread_name}");
    eprintln!("location: {location}");
    eprintln!("message:  {message}");
    eprintln!();
    eprintln!("backtrace:");
    eprintln!("{backtrace}");
    eprintln!("==================================================");
    eprintln!();

    process::exit(101);
  }));
}

/// Extracts a human-readable panic message from panic payload metadata.
fn panic_message(info: &panic::PanicHookInfo<'_>) -> String {
  if let Some(value) = info.payload().downcast_ref::<&str>() {
    return (*value).to_string();
  }
  if let Some(value) = info.payload().downcast_ref::<String>() {
    return value.clone();
  }
  "<non-string panic payload>".to_string()
}

/// Logs which profile and config file were used, when profile mode is active.
fn log_profile_source(app: &mut TerminalApp, resolved: &ResolvedCliArgs) {
  let Some(profile) = &resolved.profile else {
    return;
  };
  let path = resolved.config_path.as_ref().map_or_else(
    || "<unknown>".to_string(),
    |item| item.display().to_string(),
  );
  app.push_info(format!("Loaded profile `{profile}` from {path}"));
}

fn display_optional(value: Option<&str>) -> &str {
  value.filter(|item| !item.is_empty()).unwrap_or("-")
}

/// Converts a parsed user command into simulator actions.
///
/// Returns `true` when the UI loop should exit, otherwise `false`.
fn handle_user_command(
  command: UserCommand,
  cli_args: &CliArgs,
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  app: &mut TerminalApp,
) -> bool {
  if !matches!(&command, UserCommand::Exit) {
    app.cancel_exit_confirmation();
  }

  match command {
    UserCommand::Status => send_command(cmd_tx, SimulatorCommand::Status, app),
    UserCommand::GetConfiguration { key } => {
      send_command(cmd_tx, SimulatorCommand::GetConfiguration { key }, app)
    }
    UserCommand::SetConfiguration { key, value } => send_command(
      cmd_tx,
      SimulatorCommand::SetConfiguration { key, value },
      app,
    ),
    UserCommand::Connect { target } => {
      connect_command(target, cli_args, cmd_tx, app)
    }
    UserCommand::Disconnect => {
      send_command(cmd_tx, SimulatorCommand::Disconnect, app)
    }
    UserCommand::Boot => send_command(cmd_tx, SimulatorCommand::Boot, app),
    UserCommand::Authorize { id_token } => {
      send_command(cmd_tx, SimulatorCommand::Authorize { id_token }, app)
    }
    UserCommand::DataTransfer {
      vendor_id,
      message_id,
      data,
    } => send_command(
      cmd_tx,
      SimulatorCommand::DataTransfer {
        vendor_id,
        message_id,
        data,
      },
      app,
    ),
    UserCommand::Start {
      connector,
      id_token,
    } => send_command(
      cmd_tx,
      SimulatorCommand::StartTransaction {
        connector,
        id_token,
      },
      app,
    ),
    UserCommand::Stop { connector, reason } => send_command(
      cmd_tx,
      SimulatorCommand::StopTransaction { connector, reason },
      app,
    ),
    UserCommand::SetConnectorStatus { connector, status } => send_command(
      cmd_tx,
      SimulatorCommand::SetConnectorStatus { connector, status },
      app,
    ),
    UserCommand::GetMeasurands {
      connector,
      measurand,
    } => get_measurands_command(cmd_tx, connector, measurand, app),
    UserCommand::GetMeterValue {
      connector,
      measurand,
    } => get_meter_command(cmd_tx, connector, measurand, app),
    UserCommand::UpdateMeterValue { connector, update } => {
      update_meter_command(cmd_tx, connector, update, app)
    }
    UserCommand::SendMeterValue {
      connector,
      measurand,
      update,
    } => send_meter_command(cmd_tx, connector, measurand, update, app),
    UserCommand::Heartbeat => {
      send_command(cmd_tx, SimulatorCommand::Heartbeat, app)
    }
    UserCommand::HeartbeatStart { seconds } => {
      send_command(cmd_tx, SimulatorCommand::StartHeartbeat { seconds }, app)
    }
    UserCommand::HeartbeatStop => {
      send_command(cmd_tx, SimulatorCommand::StopHeartbeat, app)
    }
    UserCommand::Clear => {
      app.clear_logs();
      false
    }
    UserCommand::Standards => {
      app.push_info(standards_text(app.protocol()));
      false
    }
    UserCommand::Help => {
      app.push_info(help_text());
      false
    }
    UserCommand::Exit => app.request_exit(),
  }
}

fn connect_command(
  target: ConnectTarget,
  cli_args: &CliArgs,
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  app: &mut TerminalApp,
) -> bool {
  match resolve_connect_config(target, cli_args) {
    Ok(config) => send_command(
      cmd_tx,
      SimulatorCommand::Connect {
        config: config.map(Box::new),
      },
      app,
    ),
    Err(error) => {
      app.push_error(error.to_string());
      false
    }
  }
}

fn resolve_connect_config(
  target: ConnectTarget,
  cli_args: &CliArgs,
) -> Result<Option<SimulatorConnectionConfig>> {
  let resolved = match target {
    ConnectTarget::Current => return Ok(None),
    ConnectTarget::Profile { name } => {
      cli_args.resolve_profile_for_connect(&name)?
    }
    ConnectTarget::Direct { ws_url, cp_id } => {
      cli_args.resolve_direct_for_connect(ws_url, cp_id)?
    }
  };
  SimulatorConnectionConfig::from_resolved(&resolved)
}

/// Queues a generic local reading query without changing or sending values.
fn get_measurands_command(
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  connector: Option<u16>,
  measurand: Option<Measurand>,
  app: &mut TerminalApp,
) -> bool {
  send_command(
    cmd_tx,
    SimulatorCommand::GetMeasurands {
      connector,
      measurand,
    },
    app,
  )
}

/// Queues a local reading query, returning true if the simulator is unavailable.
fn get_meter_command(
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  connector: Option<u16>,
  measurand: Measurand,
  app: &mut TerminalApp,
) -> bool {
  send_command(
    cmd_tx,
    SimulatorCommand::GetMeterValue {
      connector,
      measurand,
    },
    app,
  )
}

/// Queues a local meter update, returning true if the simulator is unavailable.
fn update_meter_command(
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  connector: u16,
  update: MeterUpdate,
  app: &mut TerminalApp,
) -> bool {
  send_command(
    cmd_tx,
    SimulatorCommand::UpdateMeterValue { connector, update },
    app,
  )
}

/// Queues a send that applies an optional meter update first.
///
/// Returns `true` when the simulator task is unavailable.
fn send_meter_command(
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  connector: u16,
  measurand: Measurand,
  update: Option<MeterUpdate>,
  app: &mut TerminalApp,
) -> bool {
  send_command(
    cmd_tx,
    SimulatorCommand::SendMeterValue {
      connector,
      measurand,
      update,
    },
    app,
  )
}

/// Sends a simulator command over the command channel.
///
/// Returns `true` when the simulator task is unavailable and the caller should
/// terminate the UI loop, otherwise `false`.
fn send_command(
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  command: SimulatorCommand,
  app: &mut TerminalApp,
) -> bool {
  if cmd_tx.send(command).is_err() {
    app.push_error("Simulator task is no longer running.");
    return true;
  }
  false
}

#[cfg(test)]
mod tests {
  use tokio::sync::mpsc;

  use super::*;
  use crate::metering::MeterReading;

  #[test]
  /// Verifies connection-oriented commands dispatch the expected variants.
  fn parsed_connection_commands_dispatch_to_simulator_commands() {
    let (mut cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();
    let config_path = std::env::current_dir()
      .expect("cwd")
      .join(format!(".tmp-ocppsim-command-{}.toml", std::process::id()));
    std::fs::write(&config_path, "").expect("write empty config");
    cli_args.config_path = Some(config_path.clone());

    assert!(!dispatch("connect", &cli_args, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::Connect { config } => assert!(config.is_none()),
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch(
      "connect ws://localhost:9000/ocpp CP-TEST",
      &cli_args,
      &cmd_tx,
      &mut app,
    ));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::Connect { config } => {
        let config = config.expect("direct connect config");
        assert_eq!(config.ws_url, "ws://localhost:9000/ocpp");
        assert_eq!(config.cp_id, "CP-TEST");
      }
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch("status", &cli_args, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Status
    ));

    assert!(!dispatch("boot", &cli_args, &cmd_tx, &mut app));
    assert!(matches!(next_command(&mut cmd_rx), SimulatorCommand::Boot));

    assert!(!dispatch("disconnect", &cli_args, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Disconnect
    ));
    let _ = std::fs::remove_file(config_path);
  }

  #[test]
  /// Verifies transaction-oriented commands retain their parsed fields.
  fn parsed_transaction_commands_dispatch_to_simulator_commands() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();

    assert!(!dispatch("authorize TOKEN", &cli_args, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::Authorize { id_token } => assert_eq!(id_token, "TOKEN"),
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch(
      "data-transfer vendor message hello world",
      &cli_args,
      &cmd_tx,
      &mut app,
    ));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::DataTransfer {
        vendor_id,
        message_id,
        data,
      } => {
        assert_eq!(vendor_id, "vendor");
        assert_eq!(message_id.as_deref(), Some("message"));
        assert_eq!(data.as_deref(), Some("hello world"));
      }
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch("start 2 ID", &cli_args, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::StartTransaction {
        connector,
        id_token,
      } => {
        assert_eq!(connector, 2);
        assert_eq!(id_token, "ID");
      }
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch(
      "stop 2 EVDisconnected",
      &cli_args,
      &cmd_tx,
      &mut app,
    ));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::StopTransaction { connector, reason } => {
        assert_eq!(connector, 2);
        assert_eq!(reason.as_deref(), Some("EVDisconnected"));
      }
      other => panic!("unexpected command: {other:?}"),
    }
  }

  #[test]
  fn send_measurand_dispatches_one_update_then_send_command() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();
    for (input, update) in [
      ("send-measurand 2 Soc", None),
      (
        "send-measurand 2 Soc 75.5",
        Some(MeterUpdate::Set(MeterReading::soc(75.5).unwrap())),
      ),
      (
        "send-measurand 2 Soc -=2.5",
        Some(MeterUpdate::Adjust(
          Measurand::Soc,
          crate::metering::MeterDelta::Float(-2.5),
        )),
      ),
    ] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(next_command(&mut cmd_rx),
        SimulatorCommand::SendMeterValue {
          connector: 2, measurand: Measurand::Soc, update: actual,
        } if actual == update
      ));
      assert!(cmd_rx.try_recv().is_err());
    }
  }

  #[test]
  fn configuration_commands_dispatch_to_the_simulator() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();
    for (input, key) in [
      ("config list", None),
      ("config get HeartbeatInterval", Some("HeartbeatInterval")),
    ] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(
        next_command(&mut cmd_rx),
        SimulatorCommand::GetConfiguration { key: actual }
          if actual.as_deref() == key
      ));
    }
    assert!(!dispatch(
      r#"config set CpoName "Example Operator""#,
      &cli_args,
      &cmd_tx,
      &mut app
    ));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::SetConfiguration { key, value }
        if key == "CpoName" && value == "Example Operator"
    ));
    assert!(cmd_rx.try_recv().is_err());
  }

  #[test]
  fn generic_measurand_commands_dispatch_queries_and_shared_updates() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();
    for (input, connector, measurand) in [
      ("measurand", None, None),
      ("measurand 2", Some(2), None),
      ("measurand 2 Soc", Some(2), Some(Measurand::Soc)),
    ] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(next_command(&mut cmd_rx),
        SimulatorCommand::GetMeasurands {
          connector: actual_connector, measurand: actual,
        } if actual_connector == connector && actual == measurand
      ));
      assert!(cmd_rx.try_recv().is_err());
    }
    assert!(!dispatch(
      "measurand 2 sOC 75.5",
      &cli_args,
      &cmd_tx,
      &mut app
    ));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::UpdateMeterValue {
        connector: 2,
        update: MeterUpdate::Set(reading),
      } => assert_eq!(reading, MeterReading::soc(75.5).unwrap()),
      other => panic!("unexpected command: {other:?}"),
    }
    assert!(cmd_rx.try_recv().is_err());
  }

  #[test]
  fn meter_queries_dispatch_without_setting_or_sending() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();
    for (input, expected_connector) in [("meter", None), ("meter 2", Some(2))] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      match next_command(&mut cmd_rx) {
        SimulatorCommand::GetMeterValue {
          measurand: Measurand::EnergyActiveImportRegister,
          connector,
        } => {
          assert_eq!(connector, expected_connector);
        }
        other => panic!("unexpected command: {other:?}"),
      }
      assert!(cmd_rx.try_recv().is_err());
    }
  }

  #[test]
  /// Verifies metering, heartbeat, and connector commands dispatch correctly.
  fn parsed_runtime_commands_dispatch_to_simulator_commands() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();

    for (input, update) in [
      ("meter 2 1234", MeterUpdate::Set(MeterReading::energy(1234))),
      (
        "meter 2 +=100",
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (100).into(),
        ),
      ),
      (
        "meter 2 -=50",
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (-50).into(),
        ),
      ),
    ] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(next_command(&mut cmd_rx),
        SimulatorCommand::UpdateMeterValue { connector: 2, update: actual }
          if actual == update
      ));
    }

    for (input, update) in [
      ("send-meter 2", None),
      (
        "send-meter 2 5678",
        Some(MeterUpdate::Set(MeterReading::energy(5678))),
      ),
      (
        "send-meter 2 +=100",
        Some(MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (100).into(),
        )),
      ),
      (
        "send-meter 2 -=50",
        Some(MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (-50).into(),
        )),
      ),
    ] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(next_command(&mut cmd_rx),
        SimulatorCommand::SendMeterValue { measurand: Measurand::EnergyActiveImportRegister,  connector: 2, update: actual }
          if actual == update
      ));
      assert!(cmd_rx.try_recv().is_err());
    }

    assert!(!dispatch(
      "heartbeat start 12",
      &cli_args,
      &cmd_tx,
      &mut app,
    ));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::StartHeartbeat { seconds } => assert_eq!(seconds, 12),
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch("heartbeat", &cli_args, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Heartbeat
    ));

    assert!(!dispatch("heartbeat stop", &cli_args, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::StopHeartbeat
    ));

    assert!(!dispatch(
      "connector-status 2 Faulted",
      &cli_args,
      &cmd_tx,
      &mut app,
    ));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::SetConnectorStatus { connector, status } => {
        assert_eq!(connector, 2);
        assert_eq!(status, "Faulted");
      }
      other => panic!("unexpected command: {other:?}"),
    }
  }

  #[test]
  fn soc_commands_dispatch_queries_updates_and_atomic_sends() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();
    for (input, connector) in [("soc", None), ("soc 2", Some(2))] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(next_command(&mut cmd_rx),
        SimulatorCommand::GetMeterValue {
          connector: actual, measurand: Measurand::Soc,
        } if actual == connector
      ));
      assert!(cmd_rx.try_recv().is_err());
    }
    let delta = serde_json::Number::from_f64(-2.5).unwrap();
    for (input, update) in [
      (
        "soc 2 75.5",
        MeterUpdate::Set(MeterReading::soc(75.5).unwrap()),
      ),
      (
        "soc 2 -=2.5",
        MeterUpdate::Adjust(Measurand::Soc, delta.clone().into()),
      ),
    ] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(next_command(&mut cmd_rx),
        SimulatorCommand::UpdateMeterValue { connector: 2, update: actual }
          if actual == update
      ));
      assert!(cmd_rx.try_recv().is_err());
    }
    for (input, update) in [
      ("send-soc 2", None),
      (
        "send-soc 2 80",
        Some(MeterUpdate::Set(MeterReading::soc(80.0).unwrap())),
      ),
      (
        "send-soc 2 -=2.5",
        Some(MeterUpdate::Adjust(Measurand::Soc, delta.into())),
      ),
    ] {
      assert!(!dispatch(input, &cli_args, &cmd_tx, &mut app));
      assert!(matches!(next_command(&mut cmd_rx),
        SimulatorCommand::SendMeterValue {
          connector: 2, measurand: Measurand::Soc, update: actual,
        } if actual == update
      ));
      assert!(cmd_rx.try_recv().is_err());
    }
  }

  #[test]
  /// Verifies local UI commands do not enqueue simulator work.
  fn local_commands_do_not_dispatch_to_simulator() {
    let protocol = ocpp::OcppVersion::V2_1;
    let cli_args = base_cli_args();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let mut app = TerminalApp::new(protocol);

    assert!(!dispatch("help", &cli_args, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());

    assert!(!dispatch("standards", &cli_args, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());

    app.push_info("temporary log");
    assert!(!dispatch("clear", &cli_args, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());

    assert!(dispatch("exit", &cli_args, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());
  }

  #[test]
  /// Verifies risky process exit requires a repeated confirmation.
  fn risky_exit_requires_confirmation_and_non_exit_cancels_it() {
    let protocol = ocpp::OcppVersion::V2_1;
    let cli_args = base_cli_args();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let mut app = TerminalApp::new(protocol);
    app.apply(simulator::UiEvent::RuntimeState(
      simulator::SimulatorRuntimeState {
        active_transactions: 1,
        ..simulator::SimulatorRuntimeState::default()
      },
    ));

    assert!(!dispatch("exit", &cli_args, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());

    assert!(!dispatch("status", &cli_args, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Status
    ));

    assert!(!dispatch("exit", &cli_args, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());
    assert!(!dispatch("quit", &cli_args, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn shutdown_cancels_and_joins_a_stalled_simulator() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
      let _dropped_on_cancel = dropped_tx;
      started_tx.send(()).unwrap();
      std::future::pending::<()>().await;
    });
    started_rx.await.unwrap();

    tokio::time::timeout(
      Duration::from_secs(2),
      finish_simulator_shutdown(task),
    )
    .await
    .expect("shutdown must not wait for a stalled transport");
    assert!(
      dropped_rx.await.is_err(),
      "task must be dropped before return"
    );
  }

  fn dispatch(
    input: &str,
    cli_args: &CliArgs,
    cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
    app: &mut TerminalApp,
  ) -> bool {
    let command = parse_command(input).expect("valid command");
    handle_user_command(command, cli_args, cmd_tx, app)
  }

  fn next_command(
    cmd_rx: &mut mpsc::UnboundedReceiver<SimulatorCommand>,
  ) -> SimulatorCommand {
    cmd_rx.try_recv().expect("simulator command")
  }

  fn command_test_context() -> (
    CliArgs,
    mpsc::UnboundedSender<SimulatorCommand>,
    mpsc::UnboundedReceiver<SimulatorCommand>,
    TerminalApp,
  ) {
    let protocol = ocpp::OcppVersion::V1_6;
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let app = TerminalApp::new(protocol);
    (base_cli_args(), cmd_tx, cmd_rx, app)
  }

  fn base_cli_args() -> CliArgs {
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
}
