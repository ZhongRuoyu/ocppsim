//! A bounded copy of rendered log rows for temporary completion overlays.

use std::collections::VecDeque;

use crossterm::style::{Color, SetForegroundColor};
use ratatui::style::Style;
use ratatui::text::Span;

use super::LogEntry;
use super::render::layout::{
  MAX_COMPLETION_ROWS, PROMPT_BLOCK_LINES, display_width,
};
use crate::simulator::UiLogLevel;

pub(super) struct LogTail {
  entries: VecDeque<LogEntry>,
  rows: VecDeque<LogRow>,
  size: (u16, u16),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct LogRow {
  pub(super) text: String,
  pub(super) wrapped: bool,
  pub(super) width: usize,
}

impl LogTail {
  /// Tracks the visible log rows above the input's blank spacer.
  pub(super) fn new(size: (u16, u16)) -> Self {
    Self {
      entries: VecDeque::new(),
      rows: VecDeque::new(),
      size,
    }
  }

  /// Renders a log once, retaining the same rows that the terminal receives.
  pub(super) fn push(&mut self, entry: LogEntry) -> Vec<LogRow> {
    let rows = render_log(&entry, usize::from(self.size.0));
    self.rows.extend(rows.iter().cloned());
    self.trim_rows();
    self.entries.push_back(entry);
    // Eight complete entries cover at least eight rows at any width.
    while self.entries.len() > MAX_COMPLETION_ROWS {
      self.entries.pop_front();
    }
    rows
  }

  /// Rewraps the retained entries when the terminal size changes.
  pub(super) fn resize(&mut self, size: (u16, u16)) {
    if self.size == size {
      return;
    }
    self.size = size;
    self.rows = self
      .entries
      .iter()
      .flat_map(|entry| render_log(entry, usize::from(size.0)))
      .collect();
    self.trim_rows();
  }

  /// Discards logs when the visible terminal is cleared.
  pub(super) fn clear(&mut self) {
    self.entries.clear();
    self.rows.clear();
  }

  /// Counts owned rows above the prompt, excluding its blank spacer.
  pub(super) fn available_rows(&self) -> usize {
    self.rows.len()
  }

  /// Copies the covered rows, including any wrapped line's visible prefix.
  pub(super) fn tail(&self, count: usize) -> Vec<LogRow> {
    let mut start = self.rows.len().saturating_sub(count);
    // Replaying the prefix as well preserves reflow when the terminal is
    // resized while a wrapped line is partially covered by the overlay.
    while start > 0 && self.rows[start - 1].wrapped {
      start -= 1;
    }
    self.rows.iter().skip(start).cloned().collect()
  }

  fn trim_rows(&mut self) {
    let capacity = usize::from(self.size.1.saturating_sub(PROMPT_BLOCK_LINES));
    self.rows.drain(..self.rows.len().saturating_sub(capacity));
  }
}

/// Splits a log at terminal columns without inserting hard line breaks.
fn render_log(entry: &LogEntry, width: usize) -> Vec<LogRow> {
  let width = width.max(2);
  let prefix = format!("{} [{}] ", entry.timestamp, entry.level.label());
  let mut rows = Vec::new();
  let mut row = LogRow::default();
  for (text, color) in [
    (prefix.as_str(), Color::DarkGrey),
    (entry.message.as_str(), log_level_color(entry.level)),
  ] {
    let color = SetForegroundColor(color).to_string();
    row.text.push_str(&color);
    let span = Span::raw(text);
    for grapheme in span.styled_graphemes(Style::default()) {
      let columns = display_width(grapheme.symbol);
      if row.width + columns > width {
        row.wrapped = true;
        rows.push(row);
        row = LogRow {
          text: color.clone(),
          ..LogRow::default()
        };
      }
      row.text.push_str(grapheme.symbol);
      row.width += columns;
    }
  }
  rows.push(row);
  rows
}

fn log_level_color(level: UiLogLevel) -> Color {
  match level {
    UiLogLevel::Info => Color::White,
    UiLogLevel::Warn => Color::Yellow,
    UiLogLevel::Error => Color::Red,
    UiLogLevel::Tx => Color::Cyan,
    UiLogLevel::Rx => Color::Green,
  }
}

#[cfg(test)]
mod tests {
  use crossterm::style::{Color, SetForegroundColor};

  use super::{LogTail, render_log};
  use crate::app::LogEntry;
  use crate::simulator::UiLogLevel;

  fn entry(message: &str) -> LogEntry {
    LogEntry {
      timestamp: "t".to_string(),
      level: UiLogLevel::Rx,
      message: message.to_string(),
    }
  }

  #[test]
  fn wrapping_keeps_wide_and_combining_characters_together() {
    let rows = render_log(&entry("\u{754c}e\u{301}abcdef"), 8);
    assert_eq!(
      rows.iter().map(|row| row.width).collect::<Vec<_>>(),
      [7, 8, 1]
    );
    assert!(rows[0].wrapped);
    assert!(rows[1].wrapped);
    assert!(!rows[2].wrapped);
    assert!(rows[1].text.ends_with("\u{754c}e\u{301}abcde"));
    assert!(rows[2].text.ends_with('f'));
  }

  #[test]
  fn exact_width_does_not_add_an_empty_row() {
    // The timestamp and level take seven columns.
    let rows = render_log(&entry("abcdefghi"), 8);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].width, 8);
    assert!(!rows[1].wrapped);
  }

  #[test]
  fn wrapped_rows_keep_the_original_color_encoding() {
    let rows = render_log(&entry(&"x".repeat(80)), 20);
    let timestamp = SetForegroundColor(Color::DarkGrey).to_string();
    let message = SetForegroundColor(Color::Green).to_string();
    assert!(rows[0].text.starts_with(&timestamp));
    assert!(rows[0].text.contains(&message));
    for row in &rows[1..] {
      assert!(row.text.starts_with(&message));
    }
  }

  #[test]
  fn tail_includes_the_start_of_a_partially_covered_line() {
    let mut logs = LogTail::new((10, 24));
    logs.push(entry(&"x".repeat(18)));
    let rows = logs.tail(1);
    assert_eq!(rows.len(), 3);
    assert!(rows[0].text.contains("t [RX] "));
    assert!(!rows[2].wrapped);
  }

  #[test]
  fn resizing_rewraps_retained_entries_and_respects_available_height() {
    let mut logs = LogTail::new((80, 24));
    for index in 0..30 {
      logs.push(entry(&format!("log {index} {}", "x".repeat(12))));
    }
    assert_eq!(logs.entries.len(), 8);
    assert_eq!(logs.available_rows(), 20);
    logs.resize((10, 8));
    assert_eq!(logs.available_rows(), 4);
    let expected = render_log(logs.entries.back().unwrap(), 10);
    assert_eq!(logs.tail(1), expected);
    logs.clear();
    assert_eq!(logs.available_rows(), 0);
    logs.resize((80, 24));
    assert_eq!(logs.available_rows(), 0);
  }
}
