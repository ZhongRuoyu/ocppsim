use std::ops::RangeBounds;

use crate::ocpp::{
  OCPP_V1_6_SUPPORTED_ACTIONS, OCPP_V2_X_COMMON_SUPPORTED_ACTIONS, OcppVersion,
};

const USAGE_STATUS: &str = "Usage: status";
const USAGE_CONNECT: &str = "Usage: connect";
const USAGE_DISCONNECT: &str = "Usage: disconnect";
const USAGE_BOOT: &str = "Usage: boot";
const USAGE_AUTHORIZE: &str = "Usage: authorize <idToken>";
const USAGE_DATA_TRANSFER: &str =
  "Usage: data-transfer <vendorId> [messageId] [data...]";
const USAGE_START: &str = "Usage: start <connector> <idToken>";
const USAGE_STOP: &str = "Usage: stop <connector> [reason]";
const USAGE_CONNECTOR_STATUS: &str =
  "Usage: connector-status <connector> <status>";
const USAGE_METER: &str = "Usage: meter <connector> <wh>";
const USAGE_SEND_METER: &str = "Usage: send-meter <connector>";
const USAGE_HEARTBEAT_START: &str = "Usage: heartbeat start <seconds>";
const USAGE_HEARTBEAT_ALL: &str =
  "Usage: heartbeat | heartbeat start <seconds> | heartbeat stop";
const USAGE_CLEAR: &str = "Usage: clear";
const USAGE_STANDARDS: &str = "Usage: standards";
const USAGE_HELP: &str = "Usage: help";
const USAGE_EXIT: &str = "Usage: exit";

#[derive(Debug, Clone)]
pub enum UserCommand {
  Status,
  Connect,
  Disconnect,
  Boot,
  Authorize {
    id_token: String,
  },
  DataTransfer {
    vendor_id: String,
    message_id: Option<String>,
    data: Option<String>,
  },
  Start {
    connector: u16,
    id_token: String,
  },
  Stop {
    connector: u16,
    reason: Option<String>,
  },
  Meter {
    connector: u16,
    value_wh: i64,
  },
  SendMeter {
    connector: u16,
  },
  Heartbeat,
  HeartbeatStart {
    seconds: u64,
  },
  HeartbeatStop,
  SetConnectorStatus {
    connector: u16,
    status: String,
  },
  Clear,
  Standards,
  Help,
  Exit,
}

/// Parses one user-entered command line into a typed command variant.
///
/// Returns a descriptive usage error when argument counts or types do not
/// match command requirements.
pub fn parse_command(input: &str) -> Result<UserCommand, String> {
  let parts: Vec<&str> = input.split_whitespace().collect();
  if parts.is_empty() {
    return Err("Command is empty.".to_string());
  }

  let command = parts[0].to_ascii_lowercase();
  if let Some(result) = parse_simple_command(command.as_str(), &parts) {
    return result;
  }

  match command.as_str() {
    "authorize" => parse_authorize_command(&parts),
    "data-transfer" => parse_data_transfer_command(&parts),
    "start" => parse_start_command(&parts),
    "stop" => parse_stop_command(&parts),
    "connector-status" => parse_connector_status_command(&parts),
    "meter" => parse_meter_command(&parts),
    "send-meter" => parse_send_meter_command(&parts),
    "heartbeat" => parse_heartbeat_command(&parts),
    _ => Err(format!(
      "Unknown command `{}`. Type `help` for available commands.",
      parts[0]
    )),
  }
}

fn parse_simple_command(
  command: &str,
  parts: &[&str],
) -> Option<Result<UserCommand, String>> {
  let result = match command {
    "status" => {
      parse_zero_arg_command(parts, USAGE_STATUS, UserCommand::Status)
    }
    "connect" => {
      parse_zero_arg_command(parts, USAGE_CONNECT, UserCommand::Connect)
    }
    "disconnect" => {
      parse_zero_arg_command(parts, USAGE_DISCONNECT, UserCommand::Disconnect)
    }
    "boot" => parse_zero_arg_command(parts, USAGE_BOOT, UserCommand::Boot),
    "clear" => parse_zero_arg_command(parts, USAGE_CLEAR, UserCommand::Clear),
    "standards" => {
      parse_zero_arg_command(parts, USAGE_STANDARDS, UserCommand::Standards)
    }
    "help" | "h" | "?" => {
      parse_zero_arg_command(parts, USAGE_HELP, UserCommand::Help)
    }
    "exit" | "quit" => {
      parse_zero_arg_command(parts, USAGE_EXIT, UserCommand::Exit)
    }
    _ => return None,
  };
  Some(result)
}

fn parse_zero_arg_command(
  parts: &[&str],
  usage: &str,
  command: UserCommand,
) -> Result<UserCommand, String> {
  ensure_exact(parts, 1, usage)?;
  Ok(command)
}

fn parse_authorize_command(parts: &[&str]) -> Result<UserCommand, String> {
  ensure_exact(parts, 2, USAGE_AUTHORIZE)?;
  let id_token = parse_required(parts.get(1), USAGE_AUTHORIZE)?;
  Ok(UserCommand::Authorize {
    id_token: id_token.to_string(),
  })
}

fn parse_data_transfer_command(parts: &[&str]) -> Result<UserCommand, String> {
  ensure_range(parts, 2.., USAGE_DATA_TRANSFER)?;
  let vendor_id = parse_required(parts.get(1), USAGE_DATA_TRANSFER)?;
  let message_id = parts.get(2).map(|value| (*value).to_string());
  let data = if parts.len() > 3 {
    Some(parts[3..].join(" "))
  } else {
    None
  };
  Ok(UserCommand::DataTransfer {
    vendor_id: vendor_id.to_string(),
    message_id,
    data,
  })
}

fn parse_start_command(parts: &[&str]) -> Result<UserCommand, String> {
  ensure_exact(parts, 3, USAGE_START)?;
  let connector = parse_u16(parts.get(1), USAGE_START)?;
  let id_token = parse_required(parts.get(2), USAGE_START)?;
  Ok(UserCommand::Start {
    connector,
    id_token: id_token.to_string(),
  })
}

fn parse_stop_command(parts: &[&str]) -> Result<UserCommand, String> {
  ensure_range(parts, 2..=3, USAGE_STOP)?;
  let connector = parse_u16(parts.get(1), USAGE_STOP)?;
  let reason = parts.get(2).map(|value| (*value).to_string());
  Ok(UserCommand::Stop { connector, reason })
}

fn parse_connector_status_command(
  parts: &[&str],
) -> Result<UserCommand, String> {
  ensure_exact(parts, 3, USAGE_CONNECTOR_STATUS)?;
  let connector = parse_u16(parts.get(1), USAGE_CONNECTOR_STATUS)?;
  let status = parse_required(parts.get(2), USAGE_CONNECTOR_STATUS)?;
  Ok(UserCommand::SetConnectorStatus {
    connector,
    status: status.to_string(),
  })
}

fn parse_meter_command(parts: &[&str]) -> Result<UserCommand, String> {
  ensure_exact(parts, 3, USAGE_METER)?;
  let connector = parse_u16(parts.get(1), USAGE_METER)?;
  let value_wh = parse_i64(parts.get(2), USAGE_METER)?;
  Ok(UserCommand::Meter {
    connector,
    value_wh,
  })
}

fn parse_send_meter_command(parts: &[&str]) -> Result<UserCommand, String> {
  ensure_exact(parts, 2, USAGE_SEND_METER)?;
  let connector = parse_u16(parts.get(1), USAGE_SEND_METER)?;
  Ok(UserCommand::SendMeter { connector })
}

fn parse_heartbeat_command(parts: &[&str]) -> Result<UserCommand, String> {
  if parts.len() == 1 {
    return Ok(UserCommand::Heartbeat);
  }
  if parts.len() == 3 && parts[1].eq_ignore_ascii_case("start") {
    let seconds = parse_u64(parts.get(2), USAGE_HEARTBEAT_START)?;
    if seconds == 0 {
      return Err("Heartbeat interval must be positive.".to_string());
    }
    return Ok(UserCommand::HeartbeatStart { seconds });
  }
  if parts.len() == 2 && parts[1].eq_ignore_ascii_case("stop") {
    return Ok(UserCommand::HeartbeatStop);
  }
  Err(USAGE_HEARTBEAT_ALL.to_string())
}

/// Returns formatted interactive help text for the terminal command palette.
pub fn help_text() -> &'static str {
  "Interactive commands:
  status
    Show current simulator snapshot (connection, queue, connectors).
  connect
    Open WebSocket to CSMS and send boot/status notifications.
  disconnect
    Close the active WebSocket connection.
  boot
    Send BootNotification immediately (must be connected).
  authorize <idToken>
    Send Authorize for idToken.
  data-transfer <vendorId> [messageId] [data...]
    Send DataTransfer with optional messageId and text data.
  start <connector> <idToken>
    Start a transaction on connector (connector must be > 0).
  stop <connector> [reason]
    Stop active transaction on connector.
    Optional reason text is mapped to a valid reason for the active protocol.
  connector-status <connector> <status>
    Set local connector status and notify CSMS when connected.
    Valid statuses: Available, Preparing, Charging, SuspendedEVSE,
    SuspendedEV, Finishing, Reserved, Unavailable, Faulted, Occupied.
    Status is mapped to a valid status for the active protocol before sending.
  meter <connector> <wh>
    Set local meter counter (Wh) for connector.
  send-meter <connector>
    Send MeterValues for connector using current local meter value.
  heartbeat
    Send one Heartbeat now.
  heartbeat start <seconds>
    Start periodic heartbeats (seconds must be > 0).
  heartbeat stop
    Stop periodic heartbeats.
  clear
    Clear UI log view (does not affect simulator state).
  standards
    Show OCPP protocol/framing reference summary.
  help | h | ?
    Show this help.
  exit | quit
    Exit the simulator.
"
}

/// Returns a standards summary string for the active OCPP version.
pub fn standards_text(version: OcppVersion) -> String {
  let supported_count = match version {
    OcppVersion::V1_6 => OCPP_V1_6_SUPPORTED_ACTIONS.len(),
    OcppVersion::V2_0_1 | OcppVersion::V2_1 => {
      OCPP_V2_X_COMMON_SUPPORTED_ACTIONS.len()
    }
  };
  format!(
    "Active protocol: OCPP-J {}.
Message framing follows OCPP-J CALL/CALLRESULT/CALLERROR arrays.
Supported action set: {} actions{}.",
    version.label(),
    supported_count,
    if version == OcppVersion::V1_6 {
      ""
    } else {
      " in the 1.6-derived common subset"
    },
  )
}

/// Reads a required argument from split command parts.
///
/// Returns the command usage string as an error when missing.
fn parse_required<'a>(
  value: Option<&'a &'a str>,
  usage: &str,
) -> Result<&'a str, String> {
  value.copied().ok_or_else(|| usage.to_string())
}

/// Parses a strictly positive connector number.
fn parse_u16(value: Option<&&str>, usage: &str) -> Result<u16, String> {
  let raw = parse_required(value, usage)?;
  raw
    .parse::<u16>()
    .map_err(|_| usage.to_string())
    .and_then(|parsed| {
      if parsed == 0 {
        Err("Connector must be positive.".to_string())
      } else {
        Ok(parsed)
      }
    })
}

/// Parses an unsigned integer command argument.
fn parse_u64(value: Option<&&str>, usage: &str) -> Result<u64, String> {
  let raw = parse_required(value, usage)?;
  raw.parse::<u64>().map_err(|_| usage.to_string())
}

/// Parses a signed integer command argument.
fn parse_i64(value: Option<&&str>, usage: &str) -> Result<i64, String> {
  let raw = parse_required(value, usage)?;
  raw.parse::<i64>().map_err(|_| usage.to_string())
}

/// Validates that the command has exactly `expected` argument count.
fn ensure_exact(
  parts: &[&str],
  expected: usize,
  usage: &str,
) -> Result<(), String> {
  if parts.len() != expected {
    return Err(usage.to_string());
  }
  Ok(())
}

/// Validates that the command's argument count is within the specified range.
fn ensure_range(
  parts: &[&str],
  range: impl RangeBounds<usize>,
  usage: &str,
) -> Result<(), String> {
  if !range.contains(&parts.len()) {
    return Err(usage.to_string());
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::{UserCommand, help_text, parse_command};

  #[test]
  /// Verifies fixed-arity commands reject surplus tokens.
  fn rejects_excessive_status_arguments() {
    let result = parse_command("status foo");
    assert!(result.is_err());
  }

  #[test]
  /// Verifies fixed-arity commands reject surplus tokens.
  fn rejects_excessive_connect_arguments() {
    let result = parse_command("connect now");
    assert!(result.is_err());
  }

  #[test]
  /// Verifies `stop` accepts an optional trailing reason.
  fn accepts_stop_with_optional_reason() {
    let error = parse_command("stop 1 power loss");
    assert!(error.is_err());

    let command = parse_command("stop 1 powerloss").expect("valid command");
    match command {
      UserCommand::Stop { connector, reason } => {
        assert_eq!(connector, 1);
        assert_eq!(reason.as_deref(), Some("powerloss"));
      }
      _ => panic!("unexpected command variant"),
    }

    let command = parse_command("stop 2").expect("valid command");
    match command {
      UserCommand::Stop { connector, reason } => {
        assert_eq!(connector, 2);
        assert!(reason.is_none());
      }
      _ => panic!("unexpected command variant"),
    }
  }

  #[test]
  /// Verifies help text includes core command usage guidance.
  fn help_text_is_descriptive() {
    let help = help_text();
    assert!(help.contains("Show current simulator snapshot"));
    assert!(help.contains("connector-status <connector> <status>"));
    assert!(help.contains("Valid statuses:"));
    assert!(help.contains("exit | quit"));
  }
}
