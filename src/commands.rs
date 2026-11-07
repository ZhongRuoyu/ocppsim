use std::ops::RangeBounds;

use serde_json::Number;

use crate::metering::{MeterDelta, MeterReading, MeterUpdate};
use crate::ocpp::{
  ConfigurationKey, Measurand, OCPP_V1_6_SUPPORTED_ACTIONS,
  OCPP_V2_X_COMMON_SUPPORTED_ACTIONS, OcppVersion, validate_heartbeat_interval,
};
use crate::sensitive::{REDACTED_VALUE, redact_url_secrets};

/// Alternate command spellings shared by parsing and prefix completion.
pub(crate) const COMMAND_ALIASES: &[(&str, &str)] =
  &[("quit", "exit"), ("h", "help"), ("?", "help")];

const USAGE_STATUS: &str = "Usage: status";
const USAGE_CONFIG: &str =
  "Usage: config list | config get <key> | config set <key> <value>";
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
const USAGE_METER: &str = "Usage: meter [<connector> [<wh>|+=<wh>|-=<wh>]]";
const USAGE_SEND_METER: &str =
  "Usage: send-meter <connector> [<wh>|+=<wh>|-=<wh>]";
const USAGE_SOC: &str =
  "Usage: soc [<connector> [<percent>|+=<percent>|-=<percent>]]";
const USAGE_SEND_SOC: &str =
  "Usage: send-soc <connector> [<percent>|+=<percent>|-=<percent>]";
const USAGE_MEASURAND: &str =
  "Usage: measurand [<connector> [<measurand> [<value>]]]";
const USAGE_SEND_MEASURAND: &str =
  "Usage: send-measurand <connector> <measurand> [<value>]";
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
  GetConfiguration {
    key: Option<String>,
  },
  SetConfiguration {
    key: String,
    value: String,
  },
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
  GetMeasurands {
    connector: Option<u16>,
    measurand: Option<Measurand>,
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
  let command = COMMAND_ALIASES
    .iter()
    .find_map(|&(alias, canonical)| (alias == command).then_some(canonical))
    .unwrap_or(&command);
  if let Some(result) = parse_simple_command(command, &parts) {
    return result;
  }

  match command {
    "config" => parse_config_command(input, &parts),
    "connect" => parse_connect_command(&parts),
    "authorize" => parse_authorize_command(&parts),
    "data-transfer" => parse_data_transfer_command(&parts),
    "start" => parse_start_command(&parts),
    "stop" => parse_stop_command(&parts),
    "connector-status" => parse_connector_status_command(&parts),
    "meter" | "send-meter" | "soc" | "send-soc" => {
      parse_meter_command(&parts, command)
    }
    "measurand" => parse_measurand_command(&parts, false),
    "send-measurand" => parse_measurand_command(&parts, true),
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
    "config"
      if parts.len() >= 4
        && parts[1].eq_ignore_ascii_case("set")
        && ConfigurationKey::parse(parts[2])
          .is_some_and(ConfigurationKey::is_sensitive) =>
    {
      parts.truncate(3);
      parts.push(REDACTED_VALUE);
      parts.join(" ")
    }
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
    "help" => parse_zero_arg_command(parts, USAGE_HELP, UserCommand::Help),
    "exit" => parse_zero_arg_command(parts, USAGE_EXIT, UserCommand::Exit),
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

fn parse_config_command(
  input: &str,
  parts: &[&str],
) -> Result<UserCommand, String> {
  if input.contains(['\n', '\r']) {
    return Err("Configuration commands must fit on one line.".to_string());
  }
  let subcommand = parts.get(1).map(|value| value.to_ascii_lowercase());
  match subcommand.as_deref() {
    Some("list") if parts.len() == 2 => {
      Ok(UserCommand::GetConfiguration { key: None })
    }
    Some("get") if parts.len() == 3 => Ok(UserCommand::GetConfiguration {
      key: Some(parts[2].to_string()),
    }),
    Some("set") if parts.len() >= 4 => {
      // A JSON-quoted value may contain whitespace.
      let mut value = input;
      for part in &parts[..3] {
        value = value.trim_start().strip_prefix(part).expect("parsed token");
      }
      Ok(UserCommand::SetConfiguration {
        key: parts[2].to_string(),
        value: parse_configuration_argument(parts[2], value.trim())?,
      })
    }
    _ => Err(USAGE_CONFIG.to_string()),
  }
}

fn parse_configuration_argument(
  key: &str,
  value: &str,
) -> Result<String, String> {
  if !value.starts_with('"') {
    if value
      .chars()
      .any(|c| c.is_whitespace() || matches!(c, '"' | '\''))
    {
      return Err(format!(
        "{USAGE_CONFIG}. Expected a single value; use JSON quotes for \
        strings containing spaces or quotation marks."
      ));
    }
    return Ok(value.to_string());
  }
  if ConfigurationKey::parse(key).is_some_and(|key| !key.is_string()) {
    return Err(
      "Boolean and numeric configuration values must not be quoted."
        .to_string(),
    );
  }

  // JSON parsing checks escapes and trailing input. Keep errors value-free
  // because the argument may contain a password.
  serde_json::from_str::<String>(value).map_err(|error| {
    if error.is_eof() {
      "Unterminated JSON string; close the quote on the same line.".to_string()
    } else {
      "Expected a single JSON-quoted string with valid escape sequences."
        .to_string()
    }
  })
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

/// Parses explicit adjustments while preserving signs on absolute readings.
fn parse_meter_update(
  measurand: Measurand,
  value: Option<&&str>,
  usage: &str,
) -> Result<MeterUpdate, String> {
  let raw = parse_required(value, usage)?;
  let adjustment = raw.strip_prefix("+=").or_else(|| raw.strip_prefix("-="));
  let operand = adjustment.unwrap_or(raw);
  let subtract = raw.starts_with("-=");
  match measurand {
    Measurand::EnergyActiveImportRegister => {
      if adjustment.is_some() {
        let delta = parse_energy_adjustment(operand, subtract, usage)?;
        Ok(MeterUpdate::Adjust(measurand, delta.into()))
      } else {
        parse_i64(value, usage)
          .map(|value| MeterUpdate::Set(MeterReading::energy(value)))
      }
    }
    Measurand::Soc => {
      let value = operand.parse::<f64>().map_err(|_| usage.to_string())?;
      if adjustment.is_some() {
        let delta = Number::from_f64(if subtract { -value } else { value })
          .ok_or_else(|| "SoC adjustment must be finite.".to_string())?;
        Ok(MeterUpdate::Adjust(measurand, delta.into()))
      } else {
        MeterReading::soc(value).map(MeterUpdate::Set)
      }
    }
    _ => unreachable!("built-in meter commands use energy or SoC"),
  }
}

/// Parses an integer adjustment with checked negation and signed Wh bounds.
fn parse_energy_adjustment(
  operand: &str,
  subtract: bool,
  usage: &str,
) -> Result<i64, String> {
  // A wider intermediate lets -=9223372036854775808 represent i64::MIN.
  let value = operand.parse::<i128>().map_err(|_| usage.to_string())?;
  let delta = if subtract {
    value.checked_neg().ok_or_else(|| usage.to_string())?
  } else {
    value
  };
  i64::try_from(delta).map_err(|_| usage.to_string())
}

fn parse_measurand_command(
  parts: &[&str],
  send: bool,
) -> Result<UserCommand, String> {
  let usage = if send {
    USAGE_SEND_MEASURAND
  } else {
    USAGE_MEASURAND
  };
  ensure_range(parts, (if send { 3 } else { 1 })..=4, usage)?;
  let connector = parts
    .get(1)
    .map(|value| parse_u16(Some(value), usage))
    .transpose()?;
  let measurand = parts
    .get(2)
    .map(|name| {
      Measurand::parse_case_insensitive(name)
        .ok_or_else(|| format!("Unknown measurand `{name}`."))
    })
    .transpose()?;
  let update = parts
    .get(3)
    .map(|value| {
      parse_measurand_update(
        measurand.expect("a value follows a measurand"),
        value,
      )
    })
    .transpose()?;
  if send {
    return Ok(UserCommand::SendMeterValue {
      connector: connector.expect("send requires a connector"),
      measurand: measurand.expect("send requires a measurand"),
      update,
    });
  }
  if let Some(update) = update {
    Ok(UserCommand::UpdateMeterValue {
      connector: connector.expect("a value follows a connector"),
      update,
    })
  } else {
    Ok(UserCommand::GetMeasurands {
      connector,
      measurand,
    })
  }
}

/// Parses shared value syntax for local updates and measurement sends.
fn parse_measurand_update(
  measurand: Measurand,
  value: &str,
) -> Result<MeterUpdate, String> {
  if let Some(operand) = value.strip_prefix("+=") {
    Ok(MeterUpdate::Adjust(
      measurand,
      parse_measurement_number(operand)?.into(),
    ))
  } else if let Some(operand) = value.strip_prefix("-=") {
    let delta =
      MeterDelta::from(parse_measurement_number(operand)?).negated()?;
    Ok(MeterUpdate::Adjust(measurand, delta))
  } else {
    MeterReading::new(measurand, parse_measurement_number(value)?)
      .map(MeterUpdate::Set)
  }
}

/// Parses literal numeric values while keeping integer readings exact.
fn parse_measurement_number(raw: &str) -> Result<Number, String> {
  if let Ok(value) = raw.parse::<i64>() {
    return Ok(value.into());
  }
  if let Ok(value) = raw.parse::<u64>() {
    return Ok(value.into());
  }
  let digits = raw
    .strip_prefix('+')
    .or_else(|| raw.strip_prefix('-'))
    .unwrap_or(raw);
  if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
    return Err("Integer measurand value is out of range.".to_string());
  }
  let invalid = || "Measurand value must be a finite number.".to_string();
  let value = raw.parse::<f64>().map_err(|_| invalid())?;
  Number::from_f64(value).ok_or_else(invalid)
}

fn parse_heartbeat_command(parts: &[&str]) -> Result<UserCommand, String> {
  if parts.len() == 1 {
    return Ok(UserCommand::Heartbeat);
  }
  if parts.len() == 3 && parts[1].eq_ignore_ascii_case("start") {
    let seconds = parse_u64(parts.get(2), USAGE_HEARTBEAT_START)?;
    validate_heartbeat_interval(seconds)?;
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
  config list
    Show all configuration items with values, access modes, and origins.
  config get <key>
    Show a configuration item. Password values are masked.
  config set <key> <value>
    Set a writable item locally after validation, with origin local.
    Supply a single value; strings may be JSON-quoted.
    Use JSON quotes for strings containing spaces or quotation marks.
    Keep quotes on one line. Do not quote boolean or numeric values.
    Use Tab to complete subcommands, keys, and enumerable writable values.
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
    Set local meter counter (Wh) for connector; plain signs are absolute.
    Use +=<wh> or -=<wh> to adjust the current reading.
  send-meter <connector> [<wh>]
    Optionally update the local meter counter, then send the energy reading.
    Use +=<wh> or -=<wh> to adjust the reading before sending.
  soc
    Show local EV state of charge for all connectors, or unset if unknown.
  soc <connector>
    Show local EV state of charge for one connector.
  soc <connector> <percent>
    Set local EV state of charge (0-100 percent, decimals allowed).
    Plain signed numbers set an absolute value.
    Use +=<percent> or -=<percent> to adjust it by percentage points.
  send-soc <connector> [<percent>]
    Optionally set or adjust state of charge, then send the stored SoC.
  measurand [<connector> [<measurand> [<value>]]]
    Show set readings for all connectors, one connector, or a named measurand.
    A plain signed value sets the reading in its canonical OCPP unit.
    Use +=<value> or -=<value> to adjust a stored numeric reading.
    Display.ChargingComplete and Display.InletHot accept absolute values only.
    Measurand names are case insensitive; use Tab to complete supported names.
  send-measurand <connector> <measurand> [<value>]
    Send a stored reading, optionally setting or adjusting it first.
    Use the same names, units, and +=/-= syntax as measurand.
  heartbeat
    Send one Heartbeat now.
  heartbeat start <seconds>
    Set HeartbeatInterval with origin local and start periodic heartbeats.
    Seconds must be an integer from 1 to 2147483647.
  heartbeat stop
    Pause periodic heartbeats while retaining the configured interval.
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
  use crate::metering::{MeterDelta, MeterReading, MeterUpdate};
  use crate::ocpp::Measurand;

  #[test]
  fn aliases_parse_as_their_canonical_commands() {
    for input in ["quit", "QUIT", "QuIt"] {
      assert!(matches!(parse_command(input).unwrap(), UserCommand::Exit));
      assert_eq!(
        parse_command(&format!("{input} extra")).unwrap_err(),
        super::USAGE_EXIT
      );
    }
    for input in ["h", "H", "?"] {
      assert!(matches!(parse_command(input).unwrap(), UserCommand::Help));
      assert_eq!(
        parse_command(&format!("{input} extra")).unwrap_err(),
        super::USAGE_HELP
      );
    }
  }

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
  fn send_measurand_accepts_stored_absolute_and_relative_values() {
    for (input, measurand, update) in [
      ("send-measurand 1 soc", Measurand::Soc, None),
      (
        "SEND-MEASURAND 1 sOc 75.5",
        Measurand::Soc,
        Some(MeterUpdate::Set(MeterReading::soc(75.5).unwrap())),
      ),
      (
        "send-measurand 1 Power.Active.Setpoint -7200",
        Measurand::PowerActiveSetpoint,
        Some(MeterUpdate::Set(
          MeterReading::new(Measurand::PowerActiveSetpoint, (-7200).into())
            .unwrap(),
        )),
      ),
      (
        "send-measurand 1 Power.Active.Import +=100",
        Measurand::PowerActiveImport,
        Some(MeterUpdate::Adjust(
          Measurand::PowerActiveImport,
          MeterDelta::Integer(100),
        )),
      ),
      (
        "send-measurand 1 Energy.Active.Net -=18446744073709551615",
        Measurand::EnergyActiveNet,
        Some(MeterUpdate::Adjust(
          Measurand::EnergyActiveNet,
          MeterDelta::Integer(-i128::from(u64::MAX)),
        )),
      ),
    ] {
      assert!(matches!(parse_command(input).unwrap(),
        UserCommand::SendMeterValue {
          connector: 1, measurand: actual, update: actual_update,
        } if actual == measurand && actual_update == update
      ));
    }
  }

  #[test]
  fn send_measurand_rejects_missing_targets_and_invalid_updates() {
    for input in [
      "send-measurand",
      "send-measurand 1",
      "send-measurand 0 SoC",
      "send-measurand x SoC",
      "send-measurand 1 unknown",
      "send-measurand 1 SoC 101",
      "send-measurand 1 SoC -1",
      "send-measurand 1 Voltage NaN",
      "send-measurand 1 Current.Import +=",
      "send-measurand 1 SoC 50 extra",
    ] {
      assert!(parse_command(input).is_err(), "{input}");
    }
  }

  #[test]
  fn generic_measurand_queries_parse_optional_selectors() {
    for (input, connector, measurand) in [
      ("measurand", None, None),
      ("measurand 2", Some(2), None),
      (
        "MEASURAND 2 eNeRgY.aCtIvE.iMpOrT.rEgIsTeR",
        Some(2),
        Some(Measurand::EnergyActiveImportRegister),
      ),
      ("measurand 1 sOc", Some(1), Some(Measurand::Soc)),
    ] {
      assert!(matches!(parse_command(input).unwrap(),
        UserCommand::GetMeasurands {
          connector: actual_connector,
          measurand: actual,
        }
          if actual_connector == connector && actual == measurand
      ));
    }
  }

  #[test]
  fn generic_measurand_values_are_absolute_and_preserve_integer_precision() {
    for (input, measurand, value) in [
      ("measurand 1 soc +5", Measurand::Soc, serde_json::json!(5)),
      (
        "measurand 1 temperature -5.5",
        Measurand::Temperature,
        serde_json::json!(-5.5),
      ),
      (
        "measurand 1 Power.Active.Setpoint -7200",
        Measurand::PowerActiveSetpoint,
        serde_json::json!(-7200),
      ),
      (
        "measurand 1 Energy.Active.Import.Register 9007199254740993",
        Measurand::EnergyActiveImportRegister,
        serde_json::json!(9_007_199_254_740_993_i64),
      ),
      (
        "measurand 1 Energy.Active.Net 18446744073709551615",
        Measurand::EnergyActiveNet,
        serde_json::json!(u64::MAX),
      ),
    ] {
      match parse_command(input).unwrap() {
        UserCommand::UpdateMeterValue {
          connector: 1,
          update: MeterUpdate::Set(reading),
        } => {
          assert_eq!(reading.measurand, measurand);
          assert_eq!(serde_json::Value::Number(reading.value), value);
        }
        other => panic!("unexpected command: {other:?}"),
      }
    }
  }

  #[test]
  fn generic_adjustments_normalize_to_exact_signed_deltas() {
    for (input, expected) in [
      (
        "measurand 1 Energy.Active.Net +=9007199254740993",
        MeterDelta::Integer(9_007_199_254_740_993),
      ),
      (
        "measurand 1 Energy.Active.Net +=18446744073709551615",
        MeterDelta::Integer(i128::from(u64::MAX)),
      ),
      (
        "measurand 1 Energy.Active.Net -=18446744073709551615",
        MeterDelta::Integer(-i128::from(u64::MAX)),
      ),
      (
        "measurand 1 Energy.Active.Net -=-9223372036854775808",
        MeterDelta::Integer(-i128::from(i64::MIN)),
      ),
      ("measurand 1 SoC -=2.5", MeterDelta::Float(-2.5)),
    ] {
      match parse_command(input).unwrap() {
        UserCommand::UpdateMeterValue {
          connector: 1,
          update: MeterUpdate::Adjust(_, delta),
        } => assert_eq!(delta, expected),
        other => panic!("unexpected command: {other:?}"),
      }
    }
  }

  #[test]
  fn generic_integer_overflow_and_invalid_adjustments_are_rejected() {
    for value in ["18446744073709551616", "-9223372036854775809"] {
      for operator in ["", "+=", "-="] {
        assert!(
          parse_command(&format!(
            "measurand 1 Energy.Active.Net {operator}{value}"
          ))
          .is_err()
        );
      }
    }
    for value in ["+=", "-=", "+==1", "-=--1", "+=NaN", "-=inf", "+=1e999"] {
      assert!(
        parse_command(&format!("measurand 1 Current.Import {value}")).is_err()
      );
    }
  }

  #[test]
  fn generic_measurand_rejects_invalid_names_values_and_arity() {
    for input in [
      "measurand 0",
      "measurand nope",
      "measurand 1 unknown",
      "measurand 1 SoC 101",
      "measurand 1 SoC -1",
      "measurand 1 Power.Active.Import NaN",
      "measurand 1 Voltage inf",
      "measurand 1 Voltage 1e999",
      "measurand 1 SoC 50 extra",
      "measurand 1 SoC 50%",
      "measurand 1 Energy.Active.Import.Register 0.5",
      "measurand 1 Display.ChargingComplete 2",
      "measurand 1 Power.Factor 1.5",
    ] {
      assert!(parse_command(input).is_err(), "{input}");
    }
  }

  #[test]
  fn meter_updates_distinguish_relative_and_absolute_values() {
    let set = |value| MeterUpdate::Set(MeterReading::energy(value));
    let adjust = |value: i64| {
      MeterUpdate::Adjust(Measurand::EnergyActiveImportRegister, value.into())
    };
    for (value, expected) in [
      ("100", set(100)),
      ("+100", set(100)),
      ("-50", set(-50)),
      ("0", set(0)),
      ("+0", set(0)),
      ("-0", set(0)),
      ("+9223372036854775807", set(i64::MAX)),
      ("-9223372036854775808", set(i64::MIN)),
      ("+=100", adjust(100)),
      ("-=50", adjust(-50)),
      ("+=0", adjust(0)),
      ("-=0", adjust(0)),
      ("+=-50", adjust(-50)),
      ("-=-50", adjust(50)),
      ("-=+50", adjust(-50)),
      ("+=9223372036854775807", adjust(i64::MAX)),
      ("+=-9223372036854775808", adjust(i64::MIN)),
      ("-=9223372036854775808", adjust(i64::MIN)),
    ] {
      for name in ["meter", "send-meter"] {
        let update = match parse_command(&format!("{name} 1 {value}")).unwrap()
        {
          UserCommand::UpdateMeterValue {
            connector: 1,
            update,
          }
          | UserCommand::SendMeterValue {
            connector: 1,
            measurand: Measurand::EnergyActiveImportRegister,
            update: Some(update),
          } => update,
          other => panic!("unexpected command: {other:?}"),
        };
        assert_eq!(update, expected, "{name} 1 {value}");
      }
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
        "+=",
        "-=",
        "+=--1",
        "-=--1",
        "+==1",
        "*=1",
        "+=1.5",
        "+9223372036854775808",
        "-9223372036854775809",
        "+=9223372036854775808",
        "-=9223372036854775809",
        "-=-9223372036854775808",
        "-=-170141183460469231731687303715884105728",
      ] {
        assert!(parse_command(&format!("{command} 1 {value}")).is_err());
      }
      assert!(parse_command(&format!("{command} 1 += 5")).is_err());
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
    for percent in ["0", "+0", "-0", "75.5", "+75.5", "100"] {
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
  fn soc_commands_parse_explicit_decimal_adjustments() {
    for (input, delta) in [
      ("soc 1 +=2.5", 2.5),
      ("soc 1 -=1", -1.0),
      ("soc 1 +=0", 0.0),
      ("soc 1 -=0", 0.0),
      ("soc 1 +=-2.5", -2.5),
      ("soc 1 -=-2.5", 2.5),
      ("send-soc 1 -=2.5", -2.5),
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
        if actual == MeterDelta::Float(delta)));
    }
    for name in ["soc", "send-soc"] {
      for value in [
        "+", "-", "++1", "--1", "+-1", "-+1", "+NaN", "+inf", "+=", "-=",
        "+=--1", "-=--1", "+==1", "*=1", "+=NaN", "-=inf", "+=1e999",
      ] {
        assert!(parse_command(&format!("{name} 1 {value}")).is_err());
      }
    }
  }

  #[test]
  fn soc_commands_reject_invalid_or_out_of_range_values() {
    for name in ["soc", "send-soc"] {
      for value in [
        "-1", "-0.1", "100.1", "NaN", "inf", "-inf", "1e999", "75%", "abc",
      ] {
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
    assert!(help.contains("config list"));
    assert!(help.contains("config get <key>"));
    assert!(help.contains("config set <key> <value>"));
    assert!(help.contains("connect [<profile> | <ws-url> <cp-id>]"));
    assert!(help.contains("connector-status <connector> <status>"));
    assert!(help.contains("Valid statuses:"));
    assert!(help.contains("exit | quit"));
  }

  #[test]
  fn heartbeat_start_enforces_the_configuration_interval_range() {
    for seconds in [1, 20, 2_147_483_647] {
      assert!(matches!(
        parse_command(&format!("heartbeat start {seconds}")).unwrap(),
        UserCommand::HeartbeatStart { seconds: actual } if actual == seconds
      ));
    }
    for seconds in ["0", "-1", "1.5", "2147483648", "18446744073709551615"] {
      assert!(parse_command(&format!("heartbeat start {seconds}")).is_err());
    }
  }

  #[test]
  fn parses_configuration_commands_and_preserves_string_values() {
    assert!(matches!(
      parse_command("config list").unwrap(),
      UserCommand::GetConfiguration { key: None }
    ));
    assert!(matches!(
      parse_command("CONFIG GET HeartbeatInterval").unwrap(),
      UserCommand::GetConfiguration { key: Some(key) }
        if key == "HeartbeatInterval"
    ));
    assert!(matches!(
      parse_command("config set AuthorizeRemoteTxRequests false").unwrap(),
      UserCommand::SetConfiguration { key, value }
        if key == "AuthorizeRemoteTxRequests" && value == "false"
    ));
    assert!(matches!(
      parse_command(r#"  config   set  CpoName  "Example  Operator"  "#)
        .unwrap(),
      UserCommand::SetConfiguration { key, value }
        if key == "CpoName" && value == "Example  Operator"
    ));
    for input in [
      "config",
      "config unknown",
      "config list extra",
      "config get",
      "config get key extra",
      "config set",
      "config set key",
      "config set key   ",
    ] {
      assert_eq!(parse_command(input).unwrap_err(), super::USAGE_CONFIG);
    }
  }

  #[test]
  fn configuration_string_arguments_decode_quotes_and_escapes() {
    for (argument, expected) in [
      ("Y", "Y"),
      (r#""Y""#, "Y"),
      (r#""\"Y\"""#, r#""Y""#),
      (r#""Example  Operator""#, "Example  Operator"),
      (r#""  Example  Operator  ""#, "  Example  Operator  "),
      (r#""""#, ""),
      (r#""O'Brien""#, "O'Brien"),
      (r#""a\\b\nc\rd\te\b\f""#, "a\\b\nc\rd\te\u{8}\u{c}"),
      (r#""\u0041\u00e9""#, "A\u{e9}"),
      (r#""\uD83D\uDE42""#, "\u{1f642}"),
      (r#""https:\/\/example.test""#, "https://example.test"),
      (r#""backslash\\""#, "backslash\\"),
    ] {
      let input = format!("config set CpoName {argument}");
      assert!(
        matches!(
          parse_command(&input).unwrap(),
          UserCommand::SetConfiguration { key, value }
            if key == "CpoName" && value == expected
        ),
        "{input}"
      );
    }
    // Numeric-looking passwords and string lists still have string types.
    for key in [
      "AuthorizationKey",
      "basic-auth-password",
      "organization-name",
      "SupportedFileTransferProtocols",
    ] {
      assert!(matches!(
        parse_command(&format!(r#"config set {key} "1234""#)).unwrap(),
        UserCommand::SetConfiguration { value, .. } if value == "1234"
      ));
    }
  }

  #[test]
  fn configuration_boolean_and_numeric_arguments_must_be_unquoted() {
    for (key, value) in [
      ("AuthorizeRemoteTxRequests", "true"),
      ("AllowOfflineTxForUnknownId", "false"),
      ("AllowSecurityProfileDowngrade", "false"),
      ("AdditionalRootCertificateCheck", "true"),
      ("HeartbeatInterval", "20"),
      ("MeterValueSampleInterval", "5"),
      ("WebSocketPingInterval", "30"),
      ("CertSigningRepeatTimes", "3"),
      ("CertSigningWaitMinimum", "60"),
      ("SecurityProfile", "2"),
      ("NumberOfConnectors", "1"),
    ] {
      assert!(parse_command(&format!("config set {key} {value}")).is_ok());
      let input = format!(r#"config set {key} "{value}""#);
      assert!(
        parse_command(&input)
          .unwrap_err()
          .contains("must not be quoted")
      );
    }
  }

  #[test]
  fn configuration_arguments_reject_extra_parts_and_invalid_quotes() {
    for value in [
      "Example Operator",
      "'Y'",
      "''",
      "'",
      "'Example",
      "'Example' 'Operator'",
      r#""Example" extra"#,
      r#""Example"suffix"#,
      "'Example'suffix",
      r#""Example"'Operator'"#,
      r#""""Example""""#,
      "'''Example'''",
      "O'Brien",
      r#"a"b"#,
      r#""Example" # comment"#,
    ] {
      let input = format!("config set CpoName {value}");
      assert!(parse_command(&input).is_err(), "{input}");
    }
    for value in ["\"", r#""Example\""#, r#""Example\\"#] {
      let input = format!("config set CpoName {value}");
      assert!(parse_command(&input).unwrap_err().contains("Unterminated"));
    }
    for value in [
      r#""bad\q""#,
      r#""bad\u00xy""#,
      r#""bad\uD800""#,
      r#""bad\U00110000""#,
      r#""\U0001F642""#,
      r#""\x41""#,
      r#""\e""#,
    ] {
      let input = format!("config set CpoName {value}");
      assert!(
        parse_command(&input)
          .unwrap_err()
          .contains("escape sequence")
      );
    }
    for value in ["'Example\nOperator'", "\"Example\rOperator\"", "Y\nZ"] {
      let input = format!("config set CpoName {value}");
      assert!(parse_command(&input).unwrap_err().contains("one line"));
    }
  }

  #[test]
  fn configuration_password_commands_redact_the_entire_value() {
    for key in [
      "AuthorizationKey",
      "BasicAuthPassword",
      "basic-auth-password",
    ] {
      for value in [
        "secret with spaces",
        "'secret with spaces'",
        r#""secret\" with spaces""#,
        "'unterminated secret",
        r#""secret\q""#,
      ] {
        let input = format!("CONFIG SET {key} {value}");
        assert_eq!(
          redact_sensitive_command(&input),
          format!("CONFIG SET {key} <redacted>")
        );
        if let Err(error) = parse_command(&input) {
          assert!(!error.contains("secret"));
        }
      }
    }
    let input = "config set CpoName Example Operator";
    assert_eq!(redact_sensitive_command(input), input);
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
