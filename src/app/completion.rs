use crate::ocpp::StopReason;

const COMMAND_WORDS: &[&str] = &[
  "help",
  "standards",
  "clear",
  "exit",
  "connect",
  "disconnect",
  "status",
  "boot",
  "authorize",
  "data-transfer",
  "start",
  "stop",
  "meter",
  "send-meter",
  "heartbeat",
  "connector-status",
];
const STATUS_WORDS: &[&str] = &[
  "Available",
  "Preparing",
  "Charging",
  "SuspendedEVSE",
  "SuspendedEV",
  "Finishing",
  "Reserved",
  "Unavailable",
  "Faulted",
  "Occupied",
];
const ID_TOKEN_HINTS: &[&str] = &["TAG001", "TAG002", "REMOTE"];
const STOP_REASON_HINTS: &[&str] = &[
  StopReason::Local.as_str(),
  StopReason::Remote.as_str(),
  StopReason::EmergencyStop.as_str(),
  StopReason::PowerLoss.as_str(),
  StopReason::Other.as_str(),
];
const METER_HINTS: &[&str] = &["0", "1000", "10000"];
const HEARTBEAT_HINTS: &[&str] = &["15", "30", "60"];

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
    "authorize" => {
      seed_for_position(&parts, ends_with_space, 1, ID_TOKEN_HINTS)
    }
    "start" => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors).or_else(
        || seed_for_position(&parts, ends_with_space, 2, ID_TOKEN_HINTS),
      )
    }
    "stop" => seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
      .or_else(|| {
        seed_for_position(&parts, ends_with_space, 2, STOP_REASON_HINTS)
      }),
    "meter" => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
        .or_else(|| seed_for_position(&parts, ends_with_space, 2, METER_HINTS))
    }
    "send-meter" => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
    }
    "connector-status" => {
      seed_for_connectors(&parts, ends_with_space, 1, known_connectors)
        .or_else(|| seed_for_position(&parts, ends_with_space, 2, STATUS_WORDS))
    }
    "heartbeat" => {
      seed_for_position(&parts, ends_with_space, 1, &["start", "stop"]).or_else(
        || {
          if parts
            .get(1)
            .is_some_and(|value| value.eq_ignore_ascii_case("start"))
          {
            seed_for_position(&parts, ends_with_space, 2, HEARTBEAT_HINTS)
          } else {
            None
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
