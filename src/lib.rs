mod app;
mod args;
mod commands;
mod config;
mod embedded_schemas;
mod ocpp;
mod simulator;
mod version;

use std::backtrace::Backtrace;
use std::panic;
use std::process;
use std::thread;

use anyhow::Result;
use app::{InputAction, TerminalApp, TerminalSession, restore_console};
use args::{CliArgs, ResolvedCliArgs};
use clap::Parser;
use commands::{UserCommand, help_text, parse_command, standards_text};
use simulator::{SimulatorCommand, SimulatorConfig, run_simulator};
use tokio::sync::mpsc;
use version::version_string;

/// Runs the interactive OCPP simulator command-line application.
pub async fn run() -> Result<()> {
  install_panic_hook();
  install_rustls_provider();

  let cli = CliArgs::parse();
  let resolved = cli.resolve()?;
  let protocol = resolved.protocol;

  let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<SimulatorCommand>();
  let (ui_tx, mut ui_rx) = mpsc::unbounded_channel();

  let config = SimulatorConfig::from_resolved(&resolved);
  let simulator_task =
    tokio::spawn(run_simulator(config, cmd_rx, ui_tx, cmd_tx.clone()));

  let mut terminal = TerminalSession::new()?;
  let mut app = TerminalApp::new(protocol);
  if let Some(path) = resolved.log_path.as_ref() {
    app.enable_log_path(path)?;
    app.push_info(format!("Appending logs to {}", path.display()));
  }

  log_profile_source(&mut app, &resolved);
  app.push_info(format!(
    "Simulator ready: cp-id={} protocol={} connectors={}",
    resolved.cp_id,
    protocol.label(),
    resolved.connectors
  ));
  app.push_info(
    "Type `help` for commands. Type `connect` to open a CSMS connection.",
  );

  let mut should_exit = false;
  while !should_exit {
    while let Ok(event) = ui_rx.try_recv() {
      app.apply(event);
    }

    terminal.draw(&mut app)?;
    match terminal.poll_input(&mut app)? {
      InputAction::None => {}
      InputAction::ExitRequested => {
        should_exit = true;
      }
      InputAction::Submitted(line) => {
        if line.is_empty() {
          continue;
        }
        app.push_user_input(&line);

        match parse_command(&line) {
          Ok(command) => {
            should_exit =
              handle_user_command(command, protocol, &cmd_tx, &mut app);
          }
          Err(message) => {
            app.push_error(message);
          }
        }
      }
    }
  }

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
    let location = info
      .location()
      .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
      .unwrap_or_else(|| "<unknown>".to_string());
    let message = panic_message(info);
    let backtrace = Backtrace::force_capture();

    eprintln!();
    eprintln!("==================================================");
    eprintln!("ocppsim panic");
    eprintln!("thread:   {}", thread_name);
    eprintln!("location: {}", location);
    eprintln!("message:  {}", message);
    eprintln!();
    eprintln!("backtrace:");
    eprintln!("{}", backtrace);
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
  let path = resolved
    .config_path
    .as_ref()
    .map(|item| item.display().to_string())
    .unwrap_or_else(|| "<unknown>".to_string());
  app.push_info(format!("Loaded profile `{}` from {}", profile, path));
}

/// Converts a parsed user command into simulator actions.
///
/// Returns `true` when the UI loop should exit, otherwise `false`.
fn handle_user_command(
  command: UserCommand,
  protocol: ocpp::OcppVersion,
  cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
  app: &mut TerminalApp,
) -> bool {
  match command {
    UserCommand::Status => send_command(cmd_tx, SimulatorCommand::Status, app),
    UserCommand::Connect => {
      send_command(cmd_tx, SimulatorCommand::Connect, app)
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
      app.push_info(standards_text(protocol));
      false
    }
    UserCommand::Help => {
      app.push_info(help_text());
      false
    }
    UserCommand::Exit => true,
  }
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
  /// Verifies parsed interactive commands dispatch to simulator commands.
  fn parsed_commands_dispatch_to_simulator_commands() {
    let protocol = ocpp::OcppVersion::V2_1;
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let mut app = TerminalApp::new(protocol);

    assert!(!dispatch("connect", protocol, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Connect
    ));

    assert!(!dispatch("status", protocol, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Status
    ));

    assert!(!dispatch("boot", protocol, &cmd_tx, &mut app));
    assert!(matches!(next_command(&mut cmd_rx), SimulatorCommand::Boot));

    assert!(!dispatch("authorize TOKEN", protocol, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::Authorize { id_token } => assert_eq!(id_token, "TOKEN"),
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch(
      "data-transfer vendor message hello world",
      protocol,
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

    assert!(!dispatch("start 2 ID", protocol, &cmd_tx, &mut app));
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
      protocol,
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

    assert!(!dispatch("meter 2 1234", protocol, &cmd_tx, &mut app));
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

    assert!(!dispatch("send-meter 2", protocol, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::SendMeter { connector } => assert_eq!(connector, 2),
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch("heartbeat start 12", protocol, &cmd_tx, &mut app));
    match next_command(&mut cmd_rx) {
      SimulatorCommand::StartHeartbeat { seconds } => assert_eq!(seconds, 12),
      other => panic!("unexpected command: {other:?}"),
    }

    assert!(!dispatch("heartbeat", protocol, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Heartbeat
    ));

    assert!(!dispatch("heartbeat stop", protocol, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::StopHeartbeat
    ));

    assert!(!dispatch(
      "connector-status 2 Faulted",
      protocol,
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

    assert!(!dispatch("disconnect", protocol, &cmd_tx, &mut app));
    assert!(matches!(
      next_command(&mut cmd_rx),
      SimulatorCommand::Disconnect
    ));
  }

  #[test]
  /// Verifies local UI commands do not enqueue simulator work.
  fn local_commands_do_not_dispatch_to_simulator() {
    let protocol = ocpp::OcppVersion::V2_1;
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let mut app = TerminalApp::new(protocol);

    assert!(!dispatch("help", protocol, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());

    assert!(!dispatch("standards", protocol, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());

    app.push_info("temporary log");
    assert!(!dispatch("clear", protocol, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());

    assert!(dispatch("exit", protocol, &cmd_tx, &mut app));
    assert!(cmd_rx.try_recv().is_err());
  }

  fn dispatch(
    input: &str,
    protocol: ocpp::OcppVersion,
    cmd_tx: &mpsc::UnboundedSender<SimulatorCommand>,
    app: &mut TerminalApp,
  ) -> bool {
    let command = parse_command(input).expect("valid command");
    handle_user_command(command, protocol, cmd_tx, app)
  }

  fn next_command(
    cmd_rx: &mut mpsc::UnboundedReceiver<SimulatorCommand>,
  ) -> SimulatorCommand {
    cmd_rx.try_recv().expect("simulator command")
  }
}
