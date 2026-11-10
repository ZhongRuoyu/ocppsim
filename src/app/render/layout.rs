//! Prepared prompt text and geometry shared by drawing and restoration.

use ratatui::text::Line;

use crate::app::completion::CompletionState;
use crate::app::{TerminalApp, next_char_boundary, previous_char_boundary};

pub(in crate::app) const MAX_COMPLETION_ROWS: usize = 8;
pub(in crate::app) const PROMPT_BLOCK_LINES: u16 = 4;
const PROMPT_PREFIX_WIDTH: usize = 2;
const PROMPT_PLACEHOLDER: &str = "Command";

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PromptView {
  pub(super) input: String,
  pub(super) input_kind: InputKind,
  pub(super) input_width: usize,
  pub(super) cursor_column: usize,
  pub(super) taskbar: String,
  pub(super) taskbar_width: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum InputKind {
  Normal,
  Placeholder,
  Confirmation,
}

impl PromptView {
  /// Prepares only the text and metrics that will actually be displayed.
  pub(super) fn new(app: &TerminalApp, width: usize) -> Self {
    let content_width = width.saturating_sub(1);
    let input_limit = content_width.saturating_sub(PROMPT_PREFIX_WIDTH);
    let shadow = app.prompt_shadow();
    let text = shadow.unwrap_or_else(|| app.input());
    let cursor = shadow.map_or(app.cursor(), str::len);
    let view = visible_input_view(text, cursor, input_limit);
    let (input_kind, input) = if shadow.is_some() {
      (InputKind::Confirmation, view.text.to_string())
    } else if app.input().is_empty() {
      (
        InputKind::Placeholder,
        fit_to_width(PROMPT_PLACEHOLDER, input_limit),
      )
    } else {
      (InputKind::Normal, view.text.to_string())
    };
    let taskbar = fit_to_width(&app.taskbar_line(), content_width);
    Self {
      input_width: PROMPT_PREFIX_WIDTH + display_width(&input),
      input,
      input_kind,
      cursor_column: (PROMPT_PREFIX_WIDTH + view.cursor_width)
        .min(content_width),
      taskbar_width: display_width(&taskbar),
      taskbar,
    }
  }

  /// Locates the previously painted prompt after any terminal reflow.
  pub(super) fn clear_layout(&self, width: usize) -> ClearLayout {
    ClearLayout {
      lines_above_cursor: 1 + self.cursor_column / width.max(1),
      block_lines: 1
        + terminal_rows(self.input_width, width)
        + 1
        + terminal_rows(self.taskbar_width, width),
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CompletionMenu {
  pub(super) placement: CompletionPlacement,
  pub(super) rows: Vec<MenuRow>,
  pub(super) selected: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct MenuRow {
  pub(super) text: String,
  pub(super) width: usize,
}

impl CompletionMenu {
  /// Prepares the visible candidates after applying the scrolling margin.
  pub(super) fn new(
    state: &mut CompletionState,
    placement: CompletionPlacement,
    height: usize,
    width: usize,
  ) -> Self {
    let range = state.visible_range(height);
    let selected = state.index() - range.start;
    let rows = range
      .map(|index| {
        let label = state.candidates()[index]
          .trim_end()
          .chars()
          .filter(|ch| !ch.is_control())
          .collect::<String>();
        let text = fit_to_width(&format!("  {label}"), width.saturating_sub(1));
        MenuRow {
          width: display_width(&text),
          text,
        }
      })
      .collect();
    Self {
      placement,
      rows,
      selected,
    }
  }

  /// Checks whether replacing the menu leaves the same content covered.
  pub(super) fn same_footprint(&self, other: &Self) -> bool {
    self.placement == other.placement && self.rows.len() == other.rows.len()
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CompletionPlacement {
  Above,
  Below,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ClearLayout {
  pub(super) lines_above_cursor: usize,
  pub(super) block_lines: usize,
}

/// Counts stored physical rows at their original width or after reflow.
pub(super) fn rows_at_width(
  rows: impl Iterator<Item = (usize, bool)>,
  old_width: usize,
  width: usize,
) -> usize {
  if width == old_width {
    return rows.count();
  }
  let mut height = 0;
  let mut line_width = 0;
  for (len, wrapped) in rows {
    line_width += len;
    if !wrapped {
      height += terminal_rows(line_width, width);
      line_width = 0;
    }
  }
  height
}

struct VisibleInputView<'a> {
  text: &'a str,
  cursor_width: usize,
}

fn visible_input_view(
  input: &str,
  cursor: usize,
  width: usize,
) -> VisibleInputView<'_> {
  if width == 0 {
    return VisibleInputView {
      text: "",
      cursor_width: 0,
    };
  }

  let mut start = 0;
  while display_width(&input[start..cursor]) > width {
    start = next_char_boundary(input, start);
  }

  let mut end = input.len();
  while display_width(&input[start..end]) > width {
    end = previous_char_boundary(input, end);
  }

  VisibleInputView {
    text: &input[start..end],
    cursor_width: display_width(&input[start..cursor]).min(width),
  }
}

/// Returns the terminal columns occupied by Unicode text.
pub(in crate::app) fn display_width(input: &str) -> usize {
  Line::raw(input.to_string()).width()
}

/// Truncates display text with an ASCII suffix so it stays on one row.
pub(super) fn fit_to_width(input: &str, width: usize) -> String {
  if width == 0 {
    return String::new();
  }
  if display_width(input) <= width {
    return input.to_string();
  }

  let suffix = "...";
  let body_width = width.saturating_sub(suffix.len());
  if body_width == 0 {
    return suffix[..width].to_string();
  }

  let mut output = String::new();
  for ch in input.chars() {
    let next_width = display_width(&format!("{output}{ch}"));
    if next_width > body_width {
      break;
    }
    output.push(ch);
  }
  output.push_str(suffix);
  output
}

/// Counts the physical rows occupied by a displayed line after reflow.
pub(super) fn terminal_rows(
  display_width: usize,
  terminal_width: usize,
) -> usize {
  let terminal_width = terminal_width.max(1);
  display_width.div_ceil(terminal_width).max(1)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ocpp::OcppVersion;

  #[test]
  /// Verifies taskbar text is truncated before it can wrap.
  fn fit_to_width_uses_ascii_suffix() {
    assert_eq!(super::fit_to_width("abcdef", 4), "a...");
    assert_eq!(super::fit_to_width("abcdef", 2), "..");
  }

  #[test]
  /// Verifies prompt rendering keeps the cursor visible in narrow terminals.
  fn visible_input_view_tracks_cursor_in_narrow_width() {
    let view = visible_input_view("abcdef", 5, 3);

    assert_eq!(view.text, "cde");
    assert_eq!(view.cursor_width, 3);
  }

  #[test]
  /// Verifies prompt clearing accounts for terminal reflow after resize.
  fn prompt_clear_layout_tracks_wrapped_cursor_after_resize() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.input = "a".repeat(200);
    app.cursor = 200;
    let snapshot = PromptView::new(&app, 100);

    assert_eq!(
      snapshot.clear_layout(100),
      ClearLayout {
        lines_above_cursor: 1,
        block_lines: 4,
      }
    );
    assert_eq!(
      snapshot.clear_layout(50),
      ClearLayout {
        lines_above_cursor: 2,
        block_lines: 5,
      }
    );
  }

  #[test]
  fn overlay_height_tracks_reflow_of_an_uncovered_log_prefix() {
    let rows = [(80, true), (0, false), (16, false)];
    assert_eq!(rows_at_width(rows.into_iter(), 80, 80), 3);
    assert_eq!(rows_at_width(rows.into_iter(), 80, 40), 3);
    assert_eq!(rows_at_width(rows.into_iter(), 80, 100), 2);
  }
  #[test]
  fn prepared_prompt_retains_placeholder_and_confirmation_styles() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    let placeholder = PromptView::new(&app, 80);
    assert_eq!(placeholder.input, "Command");
    assert_eq!(placeholder.input_kind, InputKind::Placeholder);
    assert_eq!(placeholder.cursor_column, 2);

    app.input = "Command".to_string();
    app.cursor = app.input.len();
    let typed = PromptView::new(&app, 80);
    assert_eq!(typed.input, placeholder.input);
    assert_eq!(typed.input_kind, InputKind::Normal);
    assert_eq!(typed.cursor_column, 9);

    app.runtime_state.active_transactions = 1;
    assert!(!app.request_exit());
    let confirmation = PromptView::new(&app, 80);
    assert_eq!(confirmation.input_kind, InputKind::Confirmation);
    assert_eq!(confirmation.input, app.prompt_shadow().unwrap());
    assert_eq!(confirmation.cursor_column, confirmation.input_width);
  }

  #[test]
  fn prepared_menu_contains_only_the_visible_candidates_and_selection() {
    let candidates = (0..20)
      .map(|i| format!("option-{i:02}-long-value "))
      .collect();
    let mut state =
      CompletionState::new(String::new(), candidates, 0, String::new());
    for _ in 0..10 {
      state.next_value(false);
    }
    let menu =
      CompletionMenu::new(&mut state, CompletionPlacement::Below, 8, 15);
    assert_eq!(menu.rows.len(), 8);
    assert_eq!(menu.selected, 6);
    assert!(menu.rows[menu.selected].text.starts_with("  option-10"));
    assert!(
      menu
        .rows
        .iter()
        .all(|row| row.width == 14 && row.text.ends_with("..."))
    );
  }
}
