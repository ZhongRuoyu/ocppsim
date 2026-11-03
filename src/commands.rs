use std::ops::RangeBounds;

use serde_json::Number;

use crate::metering::{MeterReading, MeterUpdate};
use crate::ocpp::{
  Measurand, OCPP_V1_6_SUPPORTED_ACTIONS, OCPP_V2_X_COMMON_SUPPORTED_ACTIONS,
  OcppVersion,
};
use crate::sensitive::{REDACTED_VALUE, redact_url_secrets};

const USAGE_STATUS: &str = "Usage: status";
const USAGE_CONNECT: &str = "Usage: connect [<profile> | <ws-url> <cp-id>]";
const USAGE_DISCONNECT: &str = "Usage: disconnect";
const USAGE_BOOT: &str = "Usage: boot";
const USAGE_AUTHORIZE: &str = "Usage: authorize <idToken>";
const USAGE_DATA_TRANSFER: &str =
  "Usage: data-transfer <vendorId> [messageId] [data...]";
const USAGE_START: &str = "Usage: start <connector> <idToken>";
const USAGE_STOP: &str = "Usage: stop <connector> [reason]";
const USAGE_CONNECTOR_STATUS: &str =
  "Usage: connector-status <connector> <status>";
const USAGE_METER: &str = "Usage: meter [<connector> [<wh>|+<wh>|-<wh>]]";
const USAGE_SEND_METER: &str =
  "Usage: send-meter <connector> [<wh>|+<wh>|-<wh>]";
const USAGE_SOC: &str =
  "Usage: soc [<connector> [<percent>|+<percent>|-<percent>]]";
const USAGE_SEND_SOC: &str =
  "Usage: send-soc <connector> [<percent>|+<percent>|-<percent>]";
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
  Connect {
    target: ConnectTarget,
  },
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
  GetMeterValue {
    connector: Option<u16>,
    measurand: Measurand,
  },
  UpdateMeterValue {
    connector: u16,
    update: MeterUpdate,
  },
  SendMeterValue {
    connector: u16,
    measurand: Measurand,
    update: Option<MeterUpdate>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectTarget {
  Current,
  Profile { name: String },
  Direct { ws_url: String, cp_id: String },
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
    "connect" => parse_connect_command(&parts),
    "authorize" => parse_authorize_command(&parts),
    "data-transfer" => parse_data_transfer_command(&parts),
    "start" => parse_start_command(&parts),
    "stop" => parse_stop_command(&parts),
    "connector-status" => parse_connector_status_command(&parts),
    "meter" | "send-meter" | "soc" | "send-soc" => {
      parse_meter_command(&parts, command.as_str())
    }
    "heartbeat" => parse_heartbeat_command(&parts),
    _ => Err(format!(
      "Unknown command `{}`. Type `help` for available commands.",
      parts[0]
    )),
  }
}

/// Returns a display-safe command line with sensitive arguments masked.
pub fn redact_sensitive_command(input: &str) -> String {
  let mut parts = input.split_whitespace().collect::<Vec<_>>();
  let Some(command) = parts.first() else {
    return String::new();
  };

  match command.to_ascii_lowercase().as_str() {
    "authorize" if parts.len() >= 2 => {
      parts[1] = REDACTED_VALUE;
      parts.join(" ")
    }
    "start" if parts.len() >= 3 => {
      parts[2] = REDACTED_VALUE;
      parts.join(" ")
    }
    "connect" if parts.len() >= 2 => {
      let redacted_url = redact_url_secrets(parts[1]);
      if redacted_url == parts[1] {
        input.to_string()
      } else {
        let mut redacted_parts = parts
          .iter()
          .map(|part| (*part).to_string())
          .collect::<Vec<_>>();
        redacted_parts[1] = redacted_url;
        redacted_parts.join(" ")
      }
    }
    _ => input.to_string(),
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

fn parse_connect_command(parts: &[&str]) -> Result<UserCommand, String> {
  match parts.len() {
    1 => Ok(UserCommand::Connect {
      target: ConnectTarget::Current,
    }),
    2 => Ok(UserCommand::Connect {
      target: ConnectTarget::Profile {
        name: parts[1].to_string(),
      },
    }),
    3 => Ok(UserCommand::Connect {
      target: ConnectTarget::Direct {
        ws_url: parts[1].to_string(),
        cp_id: parts[2].to_string(),
      },
    }),
    _ => Err(USAGE_CONNECT.to_string()),
  }
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

fn parse_meter_command(
  parts: &[&str],
  command: &str,
) -> Result<UserCommand, String> {
  let (measurand, send, usage) = match command {
    "meter" => (Measurand::EnergyActiveImportRegister, false, USAGE_METER),
    "send-meter" => (
      Measurand::EnergyActiveImportRegister,
      true,
      USAGE_SEND_METER,
    ),
    "soc" => (Measurand::Soc, false, USAGE_SOC),
    "send-soc" => (Measurand::Soc, true, USAGE_SEND_SOC),
    _ => unreachable!("meter command dispatch is exhaustive"),
  };
  ensure_range(parts, (if send { 2 } else { 1 })..=3, usage)?;
  if parts.len() == 1 {
    return Ok(UserCommand::GetMeterValue {
      connector: None,
      measurand,
    });
  }
  let connector = parse_u16(parts.get(1), usage)?;
  if !send && parts.len() == 2 {
    return Ok(UserCommand::GetMeterValue {
      connector: Some(connector),
      measurand,
    });
  }
  let update = parts
    .get(2)
    .map(|value| parse_meter_update(measurand, Some(value), usage))
    .transpose()?;
  if send {
    Ok(UserCommand::SendMeterValue {
      connector,
      measurand,
      update,
    })
  } else {
    Ok(UserCommand::UpdateMeterValue {
      connector,
      update: update.expect("update commands require a value"),
    })
  }
}

/// Interprets signed values as adjustments in the measurand's native unit.
fn parse_meter_update(
  measurand: Measurand,
  value: Option<&&str>,
  usage: &str,
) -> Result<MeterUpdate, String> {
  let raw = parse_required(value, usage)?;
  let relative = raw.starts_with('+') || raw.starts_with('-');
  match measurand {
    Measurand::EnergyActiveImportRegister => {
      let value = parse_i64(value, usage)?;
      Ok(if relative {
        MeterUpdate::Adjust(measurand, value.into())
      } else {
        MeterUpdate::Set(MeterReading::energy(value))
      })
    }
    Measurand::Soc => {
      let value = raw.parse::<f64>().map_err(|_| usage.to_string())?;
      if relative {
        let delta = Number::from_f64(value)
          .ok_or_else(|| "SoC adjustment must be finite.".to_string())?;
        Ok(MeterUpdate::Adjust(measurand, delta))
      } else {
        MeterReading::soc(value).map(MeterUpdate::Set)
      }
    }
    _ => unreachable!("built-in meter commands use energy or SoC"),
  }
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
  connect [<profile> | <ws-url> <cp-id>]
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
  meter
    Show local meter counters (Wh) for all connectors.
  meter <connector>
    Show the local meter counter (Wh) for one connector.
  meter <connector> <wh>
    Set local meter counter (Wh) for connector.
    Use +<wh> or -<wh> to adjust the current reading.
  send-meter <connector> [<wh>]
    Optionally update the local meter counter, then send the energy reading.
    Use +<wh> or -<wh> to adjust the reading before sending.
  soc
    Show local EV state of charge for all connectors, or unset if unknown.
  soc <connector>
    Show local EV state of charge for one connector.
  soc <connector> <percent>
    Set local EV state of charge (0-100 percent, decimals allowed).
    Use +<percent> or -<percent> to adjust it by percentage points.
  send-soc <connector> [<percent>]
    Optionally set or adjust state of charge, then send the stored SoC.
  heartbeat
    Send one Heartbeat now.
  heartbeat start <seconds>
    Start periodic heartbeats (seconds must be > 0).
  heartbeat stop
    Stop periodic heartbeats.
  clear
    Clear the visible terminal screen (does not affect simulator state).
  standards
    Show OCPP protocol/framing reference summary.
  help | h | ?
    Show this help.
  exit | quit
    Exit the simulator. Requires confirmation when CSMS state is active.
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
  use super::{
    ConnectTarget, USAGE_SEND_METER, UserCommand, help_text, parse_command,
    redact_sensitive_command,
  };
  use crate::metering::{MeterReading, MeterUpdate};
  use crate::ocpp::Measurand;

  #[test]
  /// Verifies fixed-arity commands reject surplus tokens.
  fn rejects_excessive_status_arguments() {
    let result = parse_command("status foo");
    assert!(result.is_err());
  }

  #[test]
  /// Verifies `connect` accepts current, profile, and direct target forms.
  fn accepts_connect_target_forms() {
    let command = parse_command("connect").expect("valid command");
    assert!(matches!(
      command,
      UserCommand::Connect {
        target: ConnectTarget::Current,
      }
    ));

    let command = parse_command("connect demo").expect("valid command");
    assert!(matches!(
      command,
      UserCommand::Connect {
        target: ConnectTarget::Profile { name },
      } if name == "demo"
    ));

    let command = parse_command("connect ws://localhost:9000/ocpp CP-TEST")
      .expect("valid command");
    assert!(matches!(
      command,
      UserCommand::Connect {
        target: ConnectTarget::Direct { ws_url, cp_id },
      } if ws_url == "ws://localhost:9000/ocpp" && cp_id == "CP-TEST"
    ));

    let result = parse_command("connect one two three");
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
  fn meter_updates_distinguish_relative_and_absolute_values() {
    use crate::metering::MeterUpdate;

    for (value, expected) in [
      ("100", MeterUpdate::Set(MeterReading::energy(100))),
      (
        "+100",
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (100).into(),
        ),
      ),
      (
        "-50",
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (-50).into(),
        ),
      ),
      ("0", MeterUpdate::Set(MeterReading::energy(0))),
      (
        "+0",
        MeterUpdate::Adjust(Measurand::EnergyActiveImportRegister, (0).into()),
      ),
      (
        "-0",
        MeterUpdate::Adjust(Measurand::EnergyActiveImportRegister, (0).into()),
      ),
      (
        "+9223372036854775807",
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (i64::MAX).into(),
        ),
      ),
      (
        "-9223372036854775808",
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (i64::MIN).into(),
        ),
      ),
    ] {
      assert!(matches!(
        parse_command(&format!("meter 1 {value}")).unwrap(),
        UserCommand::UpdateMeterValue { connector: 1, update } if update == expected
      ));
      assert!(matches!(
        parse_command(&format!("send-meter 1 {value}")).unwrap(),
        UserCommand::SendMeterValue { measurand: Measurand::EnergyActiveImportRegister,  connector: 1, update: Some(update) }
          if update == expected
      ));
    }
  }

  #[test]
  fn meter_updates_reject_malformed_or_out_of_range_deltas() {
    for command in ["meter", "send-meter"] {
      for value in [
        "+",
        "-",
        "++1",
        "--1",
        "+-1",
        "-+1",
        "+1.5",
        "+9223372036854775808",
        "-9223372036854775809",
      ] {
        assert!(parse_command(&format!("{command} 1 {value}")).is_err());
      }
    }
  }

  #[test]
  fn rejects_invalid_meter_query_and_set_arguments() {
    for input in [
      "meter invalid",
      "meter 65536",
      "meter 1 invalid",
      "meter 1 12.5",
      "meter 1 100 extra",
    ] {
      assert_eq!(
        parse_command(input).unwrap_err(),
        super::USAGE_METER,
        "{input}",
      );
    }
  }

  #[test]
  /// Verifies malformed meter shorthand is rejected before dispatch.
  fn rejects_invalid_send_meter_arguments() {
    for input in [
      "send-meter",
      "send-meter invalid",
      "send-meter 1 invalid",
      "send-meter 1 12.5",
      "send-meter 1 9223372036854775808",
      "send-meter 1 100 extra",
    ] {
      assert_eq!(
        parse_command(input).unwrap_err(),
        USAGE_SEND_METER,
        "{input}"
      );
    }
  }

  #[test]
  fn soc_commands_accept_percentages_and_optional_send_values() {
    for percent in ["0", "75.5", "100"] {
      for name in ["soc", "send-soc"] {
        let command = parse_command(&format!("{name} 2 {percent}"))
          .expect("valid percentage");
        let reading = match command {
          UserCommand::UpdateMeterValue {
            connector: 2,
            update: MeterUpdate::Set(reading),
          } if name == "soc" => reading,
          UserCommand::SendMeterValue {
            connector: 2,
            measurand: crate::ocpp::Measurand::Soc,
            update: Some(MeterUpdate::Set(reading)),
          } if name == "send-soc" => reading,
          other => panic!("unexpected command: {other:?}"),
        };
        assert_eq!(reading.value.as_f64(), percent.parse::<f64>().ok());
        assert_eq!(reading.unit, "Percent");
      }
    }
    assert!(matches!(
      parse_command("send-soc 2").unwrap(),
      UserCommand::SendMeterValue {
        connector: 2,
        measurand: crate::ocpp::Measurand::Soc,
        update: None,
      }
    ));
  }

  #[test]
  fn soc_commands_parse_signed_decimal_adjustments() {
    for (input, delta) in [
      ("soc 1 +2.5", 2.5),
      ("soc 1 -1", -1.0),
      ("soc 1 +0", 0.0),
      ("soc 1 -0", 0.0),
      ("send-soc 1 -2.5", -2.5),
    ] {
      let update = match parse_command(input).unwrap() {
        UserCommand::UpdateMeterValue { update, .. }
        | UserCommand::SendMeterValue {
          update: Some(update),
          ..
        } => update,
        other => panic!("unexpected command: {other:?}"),
      };
      assert!(matches!(update, MeterUpdate::Adjust(Measurand::Soc, actual)
        if actual.as_f64() == Some(delta)));
    }
    for name in ["soc", "send-soc"] {
      for value in ["+", "-", "++1", "--1", "+-1", "-+1", "+NaN", "+inf"] {
        assert!(parse_command(&format!("{name} 1 {value}")).is_err());
      }
    }
  }

  #[test]
  fn soc_commands_reject_invalid_or_out_of_range_values() {
    for name in ["soc", "send-soc"] {
      for value in ["100.1", "NaN", "inf", "-inf", "1e999", "75%", "abc"] {
        assert!(
          parse_command(&format!("{name} 1 {value}")).is_err(),
          "{name} {value}"
        );
      }
    }
    for input in [
      "soc x 50",
      "soc 1 50 extra",
      "send-soc",
      "send-soc x",
      "send-soc 1 50 extra",
    ] {
      assert!(parse_command(input).is_err(), "{input}");
    }
  }

  #[test]
  /// Verifies help text includes core command usage guidance.
  fn help_text_is_descriptive() {
    let help = help_text();
    assert!(help.contains("Show current simulator snapshot"));
    assert!(help.contains("connect [<profile> | <ws-url> <cp-id>]"));
    assert!(help.contains("connector-status <connector> <status>"));
    assert!(help.contains("Valid statuses:"));
    assert!(help.contains("exit | quit"));
  }

  #[test]
  /// Verifies sensitive command arguments are masked for UI logging.
  fn redacts_sensitive_command_arguments() {
    assert_eq!(
      redact_sensitive_command("authorize TOKEN"),
      "authorize <redacted>"
    );
    assert_eq!(
      redact_sensitive_command("start 2 TOKEN"),
      "start 2 <redacted>"
    );
    assert_eq!(
      redact_sensitive_command(
        "connect ws://user:secret@example.test/ocpp CP-001"
      ),
      "connect ws://<redacted>@example.test/ocpp CP-001"
    );
    assert_eq!(
      redact_sensitive_command(
        "connect wss://example.test/ocpp?token=SECRET CP-001"
      ),
      "connect wss://example.test/ocpp?token=<redacted> CP-001"
    );
    assert_eq!(redact_sensitive_command("status"), "status");
  }
}
