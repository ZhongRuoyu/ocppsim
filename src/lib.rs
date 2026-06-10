#![doc = include_str!("../README.md")]

mod app;
mod args;
mod commands;
mod config;
mod embedded_schemas;
mod ocpp;
mod sensitive;
mod simulator;
mod version;

use std::backtrace::Backtrace;
use std::panic;
use std::process;
use std::thread;

use anyhow::Result;
use app::{InputAction, TerminalApp, TerminalSession, restore_console};
use args::{Cli, CliArgs, CliCommand, ResolvedCliArgs};
use clap::Parser;
use commands::{
  ConnectTarget, UserCommand, help_text, parse_command,
  redact_sensitive_command, standards_text,
};
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

  let config = SimulatorConfig::from_resolved(&resolved);
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
  let _ = simulator_task.await;
  Ok(())
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
    UserCommand::Connect { target } => {
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
    UserCommand::Meter {
      connector,
      value_wh,
    } => send_command(
      cmd_tx,
      SimulatorCommand::SetMeter {
        connector,
        value_wh,
      },
      app,
    ),
    UserCommand::SendMeter { connector } => {
      send_command(cmd_tx, SimulatorCommand::SendMeter { connector }, app)
    }
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
  Ok(SimulatorConnectionConfig::from_resolved(&resolved))
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

  #[test]
  /// Verifies connection-oriented commands dispatch the expected variants.
  fn parsed_connection_commands_dispatch_to_simulator_commands() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();

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
  /// Verifies metering, heartbeat, and connector commands dispatch correctly.
  fn parsed_runtime_commands_dispatch_to_simulator_commands() {
    let (cli_args, cmd_tx, mut cmd_rx, mut app) = command_test_context();

    assert!(!dispatch("meter 2 1234", &cli_args, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::SetMeter {
        connector,
        value_wh,
      } => {
        assert_eq!(connector, 2);
        assert_eq!(value_wh, 1234);
      }
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch("send-meter 2", &cli_args, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::SendMeter { connector } => assert_eq!(connector, 2),
      other => panic!("unexpected command: {other:?}"),
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
