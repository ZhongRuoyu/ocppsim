use crate::commands::CommandName;
use crate::ocpp::{
  ConfigurationKey, ConnectorStatus as OcppConnectorStatus, Measurand,
  OcppVersion, StopReason,
};
use crate::simulator::ConfigurationSnapshot;

const STATUS_WORDS_V1_6: &[OcppConnectorStatus] = OcppConnectorStatus::V1_6;
const STATUS_WORDS_V2_X: &[OcppConnectorStatus] = OcppConnectorStatus::V2_X;
const STOP_REASON_HINTS_V1_6: &[StopReason] = StopReason::V1_6;
const STOP_REASON_HINTS_V2_0_1: &[StopReason] = StopReason::V2_0_1;
const STOP_REASON_HINTS_V2_1: &[StopReason] = StopReason::V2_1;
const CONNECT_URL_PREFIXES: &[&str] = &["ws://", "wss://"];

#[derive(Debug, Clone)]
pub(super) struct CompletionState {
  base: String,
  suffix: String,
  candidates: Vec<String>,
  index: usize,
}

impl CompletionState {
  pub(super) fn new(
    base: String,
    candidates: Vec<String>,
    index: usize,
    suffix: String,
  ) -> Self {
    Self {
      base,
      suffix,
      candidates,
      index,
    }
  }

  pub(super) fn is_empty(&self) -> bool {
    self.candidates.is_empty()
  }

  /// Returns the completed line and cursor, retaining the following text.
  pub(super) fn value(&self) -> (String, usize) {
    let candidate = &self.candidates[self.index];
    let candidate = if self.suffix.is_empty() {
      candidate.as_str()
    } else {
      candidate.trim_end()
    };
    // A quoted list may already have its closing quote or later members.
    let candidate = if self.suffix.trim_start().starts_with(['"', ',']) {
      candidate.strip_suffix('"').unwrap_or(candidate)
    } else {
      candidate
    };
    let prefix = format!("{}{candidate}", self.base);
    let cursor = prefix.len();
    (format!("{prefix}{}", self.suffix), cursor)
  }

  /// Advances through candidates while retaining the original following text.
  pub(super) fn next_value(&mut self, reverse: bool) -> (String, usize) {
    self.index =
      next_completion_index(self.index, self.candidates.len(), reverse);
    self.value()
  }
}

/// Returns text after the word or protocol-list member being completed.
pub(super) fn completion_suffix(input: &str, cursor: usize) -> &str {
  let prefix = &input[..cursor];
  let parts = prefix.split_whitespace().collect::<Vec<_>>();
  let in_protocol_list = parts.len() >= 3
    && CommandName::parse(parts[0]) == Some(CommandName::Config)
    && parts[1].eq_ignore_ascii_case("set")
    && ConfigurationKey::parse(parts[2])
      == Some(ConfigurationKey::SupportedFileTransferProtocols)
    && (parts.len() > 3 || prefix.ends_with(char::is_whitespace));
  let remaining = &input[cursor..];
  // At the start of a quoted value, replace that whole argument. Inside it,
  // retain the closing quote and any later list members.
  if in_protocol_list && !prefix.contains('"') && remaining.starts_with('"') {
    let mut values =
      serde_json::Deserializer::from_str(remaining).into_iter::<String>();
    if values.next().is_some_and(|value| value.is_ok()) {
      return &remaining[values.byte_offset()..];
    }
  }
  let end = remaining
    .find(|ch: char| {
      ch.is_whitespace() || (in_protocol_list && matches!(ch, ',' | '"'))
    })
    .unwrap_or(remaining.len());
  &remaining[end..]
}

pub(super) fn completion_seed(
  input: &str,
  protocol: OcppVersion,
  known_connectors: &[u16],
  profile_names: &[String],
) -> Option<(String, Vec<String>)> {
  let ends_with_space = input.ends_with(char::is_whitespace);
  let parts: Vec<&str> = input.split_whitespace().collect();

  if parts.is_empty() {
    return Some((String::new(), complete_commands("")));
  }

  if parts.len() == 1 && !ends_with_space {
    return Some((String::new(), complete_commands(parts[0])));
  }

  match CommandName::parse(parts[0])? {
    CommandName::Connect => {
      seed_for_connect_targets(&parts, ends_with_space, profile_names)
    }
    CommandName::Start
    | CommandName::Meter
    | CommandName::SendMeter
    | CommandName::Soc
    | CommandName::SendSoc => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
    }
    CommandName::Stop => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors).or_else(
        || match protocol {
          OcppVersion::V1_6 => seed_for_stop_reasons(
            &parts,
            ends_with_space,
            2,
            STOP_REASON_HINTS_V1_6,
          ),
          OcppVersion::V2_0_1 => seed_for_stop_reasons(
            &parts,
            ends_with_space,
            2,
            STOP_REASON_HINTS_V2_0_1,
          ),
          OcppVersion::V2_1 => seed_for_stop_reasons(
            &parts,
            ends_with_space,
            2,
            STOP_REASON_HINTS_V2_1,
          ),
        },
      )
    }
    CommandName::Measurand | CommandName::SendMeasurand => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
        .or_else(|| seed_for_measurands(&parts, ends_with_space, protocol))
    }
    CommandName::Heartbeat => {
      seed_for_position(&parts, ends_with_space, 1, &["start", "stop"])
    }
    CommandName::ConnectorStatus => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors).or_else(
        || match protocol {
          OcppVersion::V1_6 => {
            seed_for_statuses(&parts, ends_with_space, 2, STATUS_WORDS_V1_6)
          }
          OcppVersion::V2_0_1 | OcppVersion::V2_1 => {
            seed_for_statuses(&parts, ends_with_space, 2, STATUS_WORDS_V2_X)
          }
        },
      )
    }
    _ => None,
  }
}

/// Completes configuration keys and writable values from the simulator snapshot.
pub(super) fn configuration_completion_seed(
  input: &str,
  configuration: &[ConfigurationSnapshot],
) -> Option<(String, Vec<String>)> {
  let parts = input.split_whitespace().collect::<Vec<_>>();
  if !parts
    .first()
    .is_some_and(|word| CommandName::parse(word) == Some(CommandName::Config))
  {
    return None;
  }
  let ends_with_space = input.ends_with(char::is_whitespace);
  seed_for_position(&parts, ends_with_space, 1, &["list", "get", "set"])
    .or_else(|| {
      let subcommand = parts.get(1)?;
      if !subcommand.eq_ignore_ascii_case("get")
        && !subcommand.eq_ignore_ascii_case("set")
      {
        return None;
      }
      let mut names = configuration
        .iter()
        .map(|item| item.key.as_str().to_string())
        .collect::<Vec<_>>();
      names.sort_unstable();
      seed_for_position_owned(&parts, ends_with_space, 2, &names)
    })
    .or_else(|| {
      seed_for_configuration_values(
        input,
        &parts,
        ends_with_space,
        configuration,
      )
    })
}

fn seed_for_configuration_values(
  input: &str,
  parts: &[&str],
  ends_with_space: bool,
  configuration: &[ConfigurationSnapshot],
) -> Option<(String, Vec<String>)> {
  if !parts.get(1)?.eq_ignore_ascii_case("set") {
    return None;
  }
  let key = ConfigurationKey::parse(parts.get(2)?)?;
  configuration
    .iter()
    .find(|item| item.key == key && !item.read_only)?;
  let options = key.value_options();
  if options.is_empty() {
    return None;
  }
  if key == ConfigurationKey::SupportedFileTransferProtocols {
    seed_for_configuration_list(input, parts, options)
  } else {
    if parts
      .get(3)
      .is_some_and(|value| value.starts_with(['"', '\'']))
    {
      return None;
    }
    seed_for_position(parts, ends_with_space, 3, options)
  }
}

/// Completes the current list member while retaining prior members and spacing.
fn seed_for_configuration_list(
  input: &str,
  parts: &[&str],
  options: &[&str],
) -> Option<(String, Vec<String>)> {
  let mut value = input;
  for part in &parts[..3] {
    value = value.trim_start().strip_prefix(part)?;
  }
  let value = value.trim_start();
  if value.contains(['\n', '\r']) {
    return None;
  }
  let quoted = value.starts_with('"');
  let value = value.strip_prefix('"').unwrap_or(value);
  // Only complete an unfinished token. Unquoted lists cannot contain spaces.
  if value.contains(['"', '\'', '\\'])
    || (!quoted && value.chars().any(char::is_whitespace))
  {
    return None;
  }
  let (previous, prefix) = value.rsplit_once(',').unwrap_or(("", value));
  let prefix = prefix.trim_start();
  if prefix.chars().any(char::is_whitespace) {
    return None;
  }
  let selected = previous.split(',').map(str::trim).collect::<Vec<_>>();
  if value.contains(',')
    && selected.iter().any(|item| {
      !options
        .iter()
        .any(|option| option.eq_ignore_ascii_case(item))
    })
  {
    return None;
  }
  let remaining = options
    .iter()
    .copied()
    .filter(|option| {
      !selected
        .iter()
        .any(|item| option.eq_ignore_ascii_case(item))
    })
    .collect::<Vec<_>>();
  // Leave room for another comma unless this completes the entire option set.
  let suffix = match (remaining.len(), quoted) {
    (1, true) => "\" ",
    (1, false) => " ",
    _ => "",
  };
  Some((
    input[..input.len() - prefix.len()].to_string(),
    complete_static(prefix, &remaining)
      .into_iter()
      .map(|candidate| format!("{}{suffix}", candidate.trim_end()))
      .collect(),
  ))
}

/// Completes canonical measurands defined by the active OCPP version.
fn seed_for_measurands(
  parts: &[&str],
  ends_with_space: bool,
  protocol: OcppVersion,
) -> Option<(String, Vec<String>)> {
  let words = Measurand::ALL
    .iter()
    .filter(|measurand| measurand.is_supported(protocol))
    .map(|measurand| measurand.as_str().to_string())
    .collect::<Vec<_>>();
  seed_for_position_owned(parts, ends_with_space, 2, &words)
}

/// Produces completion candidates for `connect <profile>` and direct URLs.
fn seed_for_connect_targets(
  parts: &[&str],
  ends_with_space: bool,
  profile_names: &[String],
) -> Option<(String, Vec<String>)> {
  let token_index = if ends_with_space {
    parts.len()
  } else {
    parts.len().saturating_sub(1)
  };

  if token_index != 1 {
    return None;
  }

  let base = completion_base(parts, ends_with_space);
  let prefix = if ends_with_space {
    ""
  } else {
    parts.last().copied().unwrap_or("")
  };

  let mut candidates = filter_words(prefix, profile_names)
    .into_iter()
    .map(|profile| format!("{profile} "))
    .collect::<Vec<_>>();
  candidates.extend(
    CONNECT_URL_PREFIXES
      .iter()
      .filter(|prefix_candidate| prefix_candidate.starts_with(prefix))
      .map(|prefix_candidate| (*prefix_candidate).to_string()),
  );

  Some((base, candidates))
}

/// Completes command names, revealing aliases only for a typed prefix.
fn complete_commands(prefix: &str) -> Vec<String> {
  let prefix = prefix.to_ascii_lowercase();
  let names = CommandName::ALL.iter().map(|command| command.as_str());
  let aliases = CommandName::ALL
    .iter()
    .flat_map(|command| command.aliases().iter().copied())
    .filter(|_| !prefix.is_empty());
  names
    .chain(aliases)
    .filter(|name| name.starts_with(&prefix))
    .map(|name| format!("{name} "))
    .collect()
}

/// Filters static command words by prefix and appends trailing spaces.
fn complete_static(prefix: &str, words: &[&str]) -> Vec<String> {
  let owned: Vec<String> = words.iter().map(ToString::to_string).collect();
  let mut candidates = filter_words(prefix, &owned);
  for candidate in &mut candidates {
    candidate.push(' ');
  }
  candidates
}

/// Produces completion candidates for connector-number arguments.
fn seed_for_connectors(
  parts: &[&str],
  ends_with_space: bool,
  arg_index: usize,
  known_connectors: &[u16],
) -> Option<(String, Vec<String>)> {
  let connector_words = connector_words(known_connectors);
  seed_for_position_owned(parts, ends_with_space, arg_index, &connector_words)
}

/// Produces completion candidates from a borrowed static string table.
fn seed_for_position(
  parts: &[&str],
  ends_with_space: bool,
  arg_index: usize,
  words: &[&str],
) -> Option<(String, Vec<String>)> {
  let owned: Vec<String> = words.iter().map(ToString::to_string).collect();
  seed_for_position_owned(parts, ends_with_space, arg_index, &owned)
}

/// Produces completion candidates from OCPP connector status values.
fn seed_for_statuses(
  parts: &[&str],
  ends_with_space: bool,
  arg_index: usize,
  statuses: &[OcppConnectorStatus],
) -> Option<(String, Vec<String>)> {
  let owned = statuses
    .iter()
    .map(|status| status.as_str().to_string())
    .collect::<Vec<_>>();
  seed_for_position_owned(parts, ends_with_space, arg_index, &owned)
}

/// Produces completion candidates from OCPP stop reason values.
fn seed_for_stop_reasons(
  parts: &[&str],
  ends_with_space: bool,
  arg_index: usize,
  reasons: &[StopReason],
) -> Option<(String, Vec<String>)> {
  let owned = reasons
    .iter()
    .map(|reason| reason.as_str().to_string())
    .collect::<Vec<_>>();
  seed_for_position_owned(parts, ends_with_space, arg_index, &owned)
}

/// Produces completion candidates for one argument position.
fn seed_for_position_owned(
  parts: &[&str],
  ends_with_space: bool,
  arg_index: usize,
  words: &[String],
) -> Option<(String, Vec<String>)> {
  let token_index = if ends_with_space {
    parts.len()
  } else {
    parts.len().saturating_sub(1)
  };

  if token_index != arg_index {
    return None;
  }

  let base = completion_base(parts, ends_with_space);
  let prefix = if ends_with_space {
    ""
  } else {
    parts.last().copied().unwrap_or("")
  };

  let mut candidates = filter_words(prefix, words);
  for candidate in &mut candidates {
    candidate.push(' ');
  }

  Some((base, candidates))
}

/// Returns known connector ids as completion tokens.
fn connector_words(known_connectors: &[u16]) -> Vec<String> {
  if known_connectors.is_empty() {
    return vec!["1".to_string()];
  }

  known_connectors.iter().map(ToString::to_string).collect()
}

/// Returns case-insensitive prefix matches from a candidate word list.
fn filter_words(prefix: &str, words: &[String]) -> Vec<String> {
  let prefix_lower = prefix.to_ascii_lowercase();
  words
    .iter()
    .filter(|word| word.to_ascii_lowercase().starts_with(&prefix_lower))
    .cloned()
    .collect()
}

/// Computes the next completion index for forward or reverse traversal.
fn next_completion_index(current: usize, len: usize, reverse: bool) -> usize {
  if len == 0 {
    return 0;
  }
  if reverse {
    if current == 0 { len - 1 } else { current - 1 }
  } else {
    (current + 1) % len
  }
}

/// Returns the fixed prefix preceding the currently completed token.
fn completion_base(parts: &[&str], ends_with_space: bool) -> String {
  if parts.is_empty() {
    return String::new();
  }

  if ends_with_space {
    return format!("{} ", parts.join(" "));
  }

  if parts.len() == 1 {
    return String::new();
  }

  format!("{} ", parts[..parts.len() - 1].join(" "))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_aliases_are_hidden_without_a_typed_prefix() {
    for input in ["", " ", "\t "] {
      let (_, candidates) =
        completion_seed(input, OcppVersion::V2_1, &[], &[]).unwrap();
      let canonical = CommandName::ALL
        .iter()
        .map(|command| format!("{} ", command.as_str()))
        .collect::<Vec<_>>();
      assert_eq!(candidates, canonical);
      for alias in CommandName::ALL
        .iter()
        .flat_map(|command| command.aliases())
      {
        assert!(!candidates.contains(&format!("{alias} ")));
      }
    }
  }

  #[test]
  fn typed_alias_prefixes_complete_case_insensitively() {
    for (input, expected) in [
      ("q", vec!["quit "]),
      ("qu", vec!["quit "]),
      ("qui", vec!["quit "]),
      ("QUIT", vec!["quit "]),
      ("  Qu", vec!["quit "]),
      ("h", vec!["heartbeat ", "help ", "h "]),
      ("H", vec!["heartbeat ", "help ", "h "]),
      ("?", vec!["? "]),
      ("he", vec!["heartbeat ", "help "]),
      ("e", vec!["exit "]),
      ("quits", vec![]),
      ("??", vec![]),
    ] {
      let (_, candidates) =
        completion_seed(input, OcppVersion::V2_1, &[], &[]).unwrap();
      assert_eq!(candidates, expected, "{input}");
    }
    for input in ["quit ", "h ", "? ", "help q", "exit q"] {
      assert!(
        completion_seed(input, OcppVersion::V2_1, &[], &[]).is_none(),
        "{input}"
      );
    }
  }

  #[test]
  fn configuration_completion_uses_current_supported_keys() {
    assert_eq!(complete_commands("conf"), ["config "]);
    let keys = [
      ConfigurationKey::HeartbeatInterval,
      ConfigurationKey::CpoName,
    ]
    .map(|key| ConfigurationSnapshot {
      key,
      read_only: false,
    });
    assert_eq!(
      configuration_completion_seed("config ", &keys).unwrap().1,
      ["list ", "get ", "set "]
    );
    assert_eq!(
      configuration_completion_seed("config g", &keys).unwrap(),
      ("config ".to_string(), vec!["get ".to_string()])
    );
    for command in ["get", "set"] {
      assert_eq!(
        configuration_completion_seed(&format!("config {command} "), &keys)
          .unwrap()
          .1,
        ["CpoName ", "HeartbeatInterval "]
      );
      assert_eq!(
        configuration_completion_seed(&format!("CONFIG {command} hea"), &keys)
          .unwrap()
          .1,
        ["HeartbeatInterval "]
      );
    }
    assert!(configuration_completion_seed("config list ", &keys).is_none());
    assert!(
      configuration_completion_seed("config set CpoName ", &keys).is_none()
    );
    assert_eq!(
      configuration_completion_seed("config get Meter", &keys)
        .unwrap()
        .1,
      Vec::<String>::new()
    );
  }

  #[test]
  fn configuration_value_completion_filters_enumerated_values() {
    for key in [
      ConfigurationKey::AuthorizeRemoteTxRequests,
      ConfigurationKey::AllowOfflineTxForUnknownId,
      ConfigurationKey::AllowSecurityProfileDowngrade,
    ] {
      let configuration = [ConfigurationSnapshot {
        key,
        read_only: false,
      }];
      for (prefix, expected) in [
        ("", vec!["false ", "true "]),
        ("T", vec!["true "]),
        ("fa", vec!["false "]),
        ("yes", vec![]),
      ] {
        let base = format!("CONFIG SET {} ", key.as_str().to_ascii_lowercase());
        let (actual_base, candidates) = configuration_completion_seed(
          &format!("{base}{prefix}"),
          &configuration,
        )
        .unwrap();
        assert_eq!(actual_base, base);
        assert_eq!(candidates, expected);
      }
    }
    let configuration = [ConfigurationSnapshot {
      key: ConfigurationKey::SecurityProfile,
      read_only: false,
    }];
    assert_eq!(
      configuration_completion_seed(
        "config set SecurityProfile ",
        &configuration
      )
      .unwrap()
      .1,
      ["0 ", "1 ", "2 ", "3 "]
    );
  }

  #[test]
  fn configuration_list_completion_preserves_prior_members_and_spacing() {
    let configuration = [ConfigurationSnapshot {
      key: ConfigurationKey::SupportedFileTransferProtocols,
      read_only: false,
    }];
    let command = "config set SupportedFileTransferProtocols ";
    for (value, base_suffix, expected) in [
      ("", "", vec!["HTTP", "HTTPS", "FTP", "FTPS"]),
      ("ht", "", vec!["HTTP", "HTTPS"]),
      ("ftpS", "", vec!["FTPS"]),
      ("HTTP,", "HTTP,", vec!["HTTPS", "FTP", "FTPS"]),
      (r#""http,  ft"#, r#""http,  "#, vec!["FTP", "FTPS"]),
      (r#""HTTP, "#, r#""HTTP, "#, vec!["HTTPS", "FTP", "FTPS"]),
      (r#""ht"#, "\"", vec!["HTTP", "HTTPS"]),
      (r#""ftpS"#, "\"", vec!["FTPS"]),
      ("HTTP,HTTPS,FTPS,F", "HTTP,HTTPS,FTPS,", vec!["FTP "]),
      (
        r#""HTTP,HTTPS,FTPS,F"#,
        r#""HTTP,HTTPS,FTPS,"#,
        vec!["FTP\" "],
      ),
      (
        r#""http,ftp,https, "#,
        r#""http,ftp,https, "#,
        vec!["FTPS\" "],
      ),
      ("HTTP,HTTPS,FTP,FTPS,", "HTTP,HTTPS,FTP,FTPS,", vec![]),
    ] {
      let (base, candidates) = configuration_completion_seed(
        &format!("{command}{value}"),
        &configuration,
      )
      .unwrap();
      assert_eq!(base, format!("{command}{base_suffix}"));
      assert_eq!(candidates, expected);
      for candidate in candidates {
        let mut completed = format!("{base}{candidate}");
        if value.starts_with('"') && !candidate.ends_with("\" ") {
          assert!(crate::commands::parse_command(&completed).is_err());
          completed.push('"');
        }
        assert!(
          crate::commands::parse_command(&completed).is_ok(),
          "{completed}"
        );
      }
    }
    for value in [
      ",H",
      "HTTP,,H",
      "SMTP,H",
      "HTTP garbage",
      "HTTP ",
      "http,  ft",
      "HTTP, ",
      "'ht",
      "'HTTP' ",
      "'HTTP'",
      r#""HTTP""#,
      "'HTTP' F",
      "'HTTP\n,F",
    ] {
      assert!(
        configuration_completion_seed(
          &format!("{command}{value}"),
          &configuration,
        )
        .is_none()
      );
    }
  }

  #[test]
  fn configuration_value_completion_requires_a_writable_enumerated_item() {
    let configuration = [
      (ConfigurationKey::AdditionalRootCertificateCheck, true),
      (ConfigurationKey::SecurityProfile, true),
      (ConfigurationKey::AuthorizeRemoteTxRequests, false),
      (ConfigurationKey::BasicAuthPassword, false),
      (ConfigurationKey::HeartbeatInterval, false),
      (ConfigurationKey::CpoName, false),
    ]
    .map(|(key, read_only)| ConfigurationSnapshot { key, read_only });
    for input in [
      "config set AdditionalRootCertificateCheck ",
      "config set SecurityProfile ",
      "config set BasicAuthPassword ",
      "config set HeartbeatInterval ",
      "config set CpoName ",
      "config set UnsupportedKey ",
      "config set SupportedFileTransferProtocols ",
      "config get AuthorizeRemoteTxRequests ",
      "config set AuthorizeRemoteTxRequests true ",
      "config set AuthorizeRemoteTxRequests true false",
      "config set AuthorizeRemoteTxRequests 'tr",
      "config set AuthorizeRemoteTxRequests \"tr",
    ] {
      assert!(
        configuration_completion_seed(input, &configuration).is_none(),
        "{input}"
      );
    }
    let mut cycle = CompletionState::new(
      "config set AuthorizeRemoteTxRequests ".to_string(),
      vec!["false ".to_string(), "true ".to_string()],
      0,
      String::new(),
    );
    assert_eq!(
      cycle.next_value(false),
      (
        "config set AuthorizeRemoteTxRequests true ".to_string(),
        "config set AuthorizeRemoteTxRequests true ".len(),
      )
    );
    assert_eq!(
      cycle.next_value(true),
      (
        "config set AuthorizeRemoteTxRequests false ".to_string(),
        "config set AuthorizeRemoteTxRequests false ".len(),
      )
    );
  }

  #[test]
  fn generic_measurand_completion_uses_active_protocol_and_canonical_names() {
    assert_eq!(complete_commands("meas"), ["measurand "]);
    assert_eq!(complete_commands("send-meas"), ["send-measurand "]);
    for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1]
    {
      for command in ["measurand", "send-measurand"] {
        let (_, connectors) =
          completion_seed(&format!("{command} "), protocol, &[1, 2], &[])
            .unwrap();
        assert_eq!(connectors, ["1 ", "2 "]);
        let (_, names) =
          completion_seed(&format!("{command} 1 "), protocol, &[1, 2], &[])
            .unwrap();
        assert_eq!(
          names.contains(&"Temperature ".to_string()),
          protocol == OcppVersion::V1_6
        );
        assert_eq!(
          names.contains(&"Energy.Apparent.Net ".to_string()),
          protocol != OcppVersion::V1_6
        );
        assert_eq!(
          names.contains(&"Display.PresentSOC ".to_string()),
          protocol == OcppVersion::V2_1
        );
        let (_, names) = completion_seed(
          &format!("{} 1 eNeRgY.aCtIvE.iMpOrT.r", command.to_uppercase()),
          protocol,
          &[1],
          &[],
        )
        .unwrap();
        assert_eq!(names, ["Energy.Active.Import.Register "]);
        assert!(
          completion_seed(&format!("{command} 1 SoC "), protocol, &[1], &[],)
            .is_none()
        );
      }
    }
  }

  #[test]
  fn metering_commands_share_connector_completion() {
    for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1]
    {
      for command in ["meter", "send-meter", "soc", "send-soc"] {
        let (_, candidates) =
          completion_seed(&format!("{command} "), protocol, &[1, 2], &[])
            .expect("connector candidates");
        assert_eq!(candidates, ["1 ", "2 "]);
        let (_, candidates) =
          completion_seed(&format!("{command} 2"), protocol, &[1, 2], &[])
            .expect("matching connector");
        assert_eq!(candidates, ["2 "]);
        assert!(
          completion_seed(&format!("{command} 2 "), protocol, &[1, 2], &[],)
            .is_none()
        );
      }
    }
    assert_eq!(complete_commands("soc"), ["soc "]);
    assert_eq!(complete_commands("send-soc"), ["send-soc "]);
  }

  #[test]
  /// Verifies status completions only suggest values valid for the protocol.
  fn status_completion_is_protocol_specific() {
    let v1_6_words = completion_words("connector-status 1 ", OcppVersion::V1_6);
    assert!(v1_6_words.contains(&"Charging".to_string()));
    assert!(!v1_6_words.contains(&"Occupied".to_string()));

    let v2_1_words = completion_words("connector-status 1 ", OcppVersion::V2_1);
    assert!(v2_1_words.contains(&"Occupied".to_string()));
    assert!(!v2_1_words.contains(&"Charging".to_string()));
  }

  #[test]
  /// Verifies stop reason completions track per-version schema values.
  fn stop_reason_completion_is_protocol_specific() {
    let v1_6_words = completion_words("stop 1 ", OcppVersion::V1_6);
    assert!(v1_6_words.contains(&"UnlockCommand".to_string()));
    assert!(!v1_6_words.contains(&"Timeout".to_string()));

    let v2_0_1_words = completion_words("stop 1 ", OcppVersion::V2_0_1);
    assert!(v2_0_1_words.contains(&"Timeout".to_string()));
    assert!(!v2_0_1_words.contains(&"ReqEnergyTransferRejected".to_string(),));

    let v2_1_words = completion_words("stop 1 ", OcppVersion::V2_1);
    assert!(v2_1_words.contains(&"ReqEnergyTransferRejected".to_string(),));
  }

  #[test]
  /// Verifies connect target completions include profiles and URL prefixes.
  fn connect_completion_includes_profiles_and_url_prefixes() {
    let profiles = vec!["demo".to_string(), "yard".to_string()];
    let (_, candidates) =
      completion_seed("connect ", OcppVersion::V1_6, &[1], &profiles)
        .expect("completion candidates");

    assert_eq!(
      candidates,
      vec![
        "demo ".to_string(),
        "yard ".to_string(),
        "ws://".to_string(),
        "wss://".to_string(),
      ]
    );
  }

  #[test]
  /// Verifies connect target completions filter profile and URL prefixes.
  fn connect_completion_filters_profiles_and_url_prefixes() {
    let profiles = vec!["demo".to_string(), "yard".to_string()];
    let (_, profile_candidates) =
      completion_seed("connect de", OcppVersion::V1_6, &[1], &profiles)
        .expect("profile candidates");
    assert_eq!(profile_candidates, vec!["demo ".to_string()]);

    let (_, url_candidates) =
      completion_seed("connect w", OcppVersion::V1_6, &[1], &profiles)
        .expect("url candidates");
    assert_eq!(
      url_candidates,
      vec!["ws://".to_string(), "wss://".to_string()]
    );
  }

  fn completion_words(input: &str, protocol: OcppVersion) -> Vec<String> {
    let (_, candidates) = completion_seed(input, protocol, &[1], &[])
      .expect("completion candidates");
    candidates
      .into_iter()
      .map(|candidate| candidate.trim_end().to_string())
      .collect()
  }
}
