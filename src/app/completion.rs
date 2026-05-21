use crate::ocpp::{
  ConnectorStatus as OcppConnectorStatus, OcppVersion, StopReason,
};

const COMMAND_WORDS: &[&str] = &[
  "status",
  "connect",
  "disconnect",
  "boot",
  "authorize",
  "data-transfer",
  "start",
  "stop",
  "meter",
  "send-meter",
  "heartbeat",
  "connector-status",
  "clear",
  "standards",
  "help",
  "exit",
];
const STATUS_WORDS_V1_6: &[OcppConnectorStatus] = OcppConnectorStatus::V1_6;
const STATUS_WORDS_V2_X: &[OcppConnectorStatus] = OcppConnectorStatus::V2_X;
const STOP_REASON_HINTS_V1_6: &[StopReason] = StopReason::V1_6;
const STOP_REASON_HINTS_V2_0_1: &[StopReason] = StopReason::V2_0_1;
const STOP_REASON_HINTS_V2_1: &[StopReason] = StopReason::V2_1;

#[derive(Debug, Clone)]
pub(super) struct CompletionState {
  base: String,
  candidates: Vec<String>,
  index: usize,
}

impl CompletionState {
  pub(super) fn new(
    base: String,
    candidates: Vec<String>,
    index: usize,
  ) -> Self {
    Self {
      base,
      candidates,
      index,
    }
  }

  pub(super) fn is_empty(&self) -> bool {
    self.candidates.is_empty()
  }

  pub(super) fn next_value(&mut self, reverse: bool) -> String {
    self.index =
      next_completion_index(self.index, self.candidates.len(), reverse);
    format!("{}{}", self.base, self.candidates[self.index])
  }
}

pub(super) fn completion_seed(
  input: &str,
  protocol: OcppVersion,
  known_connectors: &[u16],
) -> Option<(String, Vec<String>)> {
  let ends_with_space = input.ends_with(char::is_whitespace);
  let parts: Vec<&str> = input.split_whitespace().collect();

  if parts.is_empty() {
    return Some((String::new(), complete_static("", COMMAND_WORDS)));
  }

  if parts.len() == 1 && !ends_with_space {
    return Some((String::new(), complete_static(parts[0], COMMAND_WORDS)));
  }

  let command = parts[0].to_ascii_lowercase();

  match command.as_str() {
    "start" => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
    }
    "stop" => seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
      .or_else(|| match protocol {
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
      }),
    "meter" => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
    }
    "send-meter" => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
    }
    "heartbeat" => {
      seed_for_position(&parts, ends_with_space, 1, &["start", "stop"])
    }
    "connector-status" => {
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

/// Filters static command words by prefix and appends trailing spaces.
fn complete_static(prefix: &str, words: &[&str]) -> Vec<String> {
  let owned: Vec<String> = words.iter().map(|word| word.to_string()).collect();
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
  seed_for_position_owned(parts, ends_with_space, arg_index, connector_words)
}

/// Produces completion candidates from a borrowed static string table.
fn seed_for_position(
  parts: &[&str],
  ends_with_space: bool,
  arg_index: usize,
  words: &[&str],
) -> Option<(String, Vec<String>)> {
  let owned: Vec<String> =
    words.iter().map(|value| value.to_string()).collect();
  seed_for_position_owned(parts, ends_with_space, arg_index, owned)
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
    .collect();
  seed_for_position_owned(parts, ends_with_space, arg_index, owned)
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
    .collect();
  seed_for_position_owned(parts, ends_with_space, arg_index, owned)
}

/// Produces completion candidates for one argument position.
fn seed_for_position_owned(
  parts: &[&str],
  ends_with_space: bool,
  arg_index: usize,
  words: Vec<String>,
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

  let mut candidates = filter_words(prefix, &words);
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

  known_connectors.iter().map(|id| id.to_string()).collect()
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

  fn completion_words(input: &str, protocol: OcppVersion) -> Vec<String> {
    let (_, candidates) =
      completion_seed(input, protocol, &[1]).expect("completion candidates");
    candidates
      .into_iter()
      .map(|candidate| candidate.trim_end().to_string())
      .collect()
  }
}
