//! Inline prompt rendering and temporary completion overlays.

use std::io::{self, Write};

use crossterm::cursor::{
  MoveDown, MoveTo, MoveToColumn, MoveUp, RestorePosition, SavePosition,
  SetCursorStyle, Show,
};
use crossterm::queue;
use crossterm::style::{
  Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor,
};
use crossterm::terminal::{
  BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate,
};

pub(super) mod layout;

use super::log_tail::{LogRow, LogTail};
use super::{LogEntry, TerminalApp};
use layout::{
  ClearLayout, CompletionMenu, CompletionPlacement, InputKind,
  MAX_COMPLETION_ROWS, PROMPT_BLOCK_LINES, PromptView, rows_at_width,
  terminal_rows,
};

pub(super) struct PromptRenderer<W: Write> {
  pub(super) stdout: W,
  rendered: Option<RenderedFrame>,
  cursor_row: Option<usize>,
  logs: LogTail,
}

impl<W: Write> PromptRenderer<W> {
  /// Creates an inline renderer without changing terminal modes.
  pub(super) fn new(stdout: W) -> Self {
    Self {
      stdout,
      rendered: None,
      cursor_row: None,
      logs: LogTail::new((80, 24)),
    }
  }

  /// Records the cursor's physical row before rendering or after a resize.
  pub(super) fn set_cursor_row(&mut self, row: Option<u16>) {
    self.cursor_row = row.map(usize::from);
  }

  /// Requests a fresh cursor position when terminal reflow can move the input.
  pub(super) fn needs_cursor_position(&self, size: (u16, u16)) -> bool {
    self
      .rendered
      .as_ref()
      .is_some_and(|frame| frame.size != (size.0.max(2), size.1.max(1)))
  }

  fn advance_cursor(&mut self, rows: usize, height: u16) {
    self.cursor_row = self.cursor_row.map(|row| {
      row
        .saturating_add(rows)
        .min(usize::from(height.saturating_sub(1)))
    });
  }

  /// Prepares the displayed view once, then repaints or rebuilds its frame.
  pub(super) fn draw(
    &mut self,
    app: &mut TerminalApp,
    size: (u16, u16),
  ) -> io::Result<()> {
    let size = (size.0.max(2), size.1.max(1));
    if !app.take_view_dirty()
      && self
        .rendered
        .as_ref()
        .is_some_and(|frame| frame.size == size)
    {
      return Ok(());
    }
    let prompt = PromptView::new(app, usize::from(size.0));
    let clear_screen = app.take_screen_clear_requested();
    let logs = app.drain_pending_logs();
    let layout_changed = clear_screen
      || !logs.is_empty()
      || self
        .rendered
        .as_ref()
        .is_none_or(|frame| frame.size != size);
    // New logs and resizing can move the input. Prepare their menu only
    // after rebuilding the prompt; otherwise its position is already known.
    let mut menu = if layout_changed {
      None
    } else {
      self.prepare_completion(app, &prompt, size)
    };
    if !layout_changed
      && self.rendered.as_ref().is_some_and(|frame| {
        frame.prompt == prompt
          && frame.overlay.as_ref().map(|overlay| &overlay.menu)
            == menu.as_ref()
      })
    {
      return Ok(());
    }
    let repaint = !layout_changed
      && self
        .rendered
        .as_ref()
        .is_some_and(|frame| frame.can_repaint(&prompt, menu.as_ref()));

    let result = self.with_synchronized_update(|renderer| {
      if repaint {
        renderer.render_input(&prompt)?;
      } else {
        renderer.clear_prompt(size)?;
        renderer.logs.resize(size);
        if clear_screen {
          queue!(renderer.stdout, Clear(ClearType::All), MoveTo(0, 0))?;
          renderer.logs.clear();
          renderer.cursor_row = Some(0);
        }
        for entry in logs {
          renderer.write_log_entry(entry, size.1)?;
        }
        renderer.render_prompt(&prompt)?;
        renderer.advance_cursor(usize::from(PROMPT_BLOCK_LINES - 1), size.1);
        renderer.cursor_row =
          renderer.cursor_row.map(|row| row.saturating_sub(2));
        if layout_changed {
          menu = renderer.prepare_completion(app, &prompt, size);
        }
      }
      let overlay = renderer.rendered.take().and_then(|frame| frame.overlay);
      let mut frame = RenderedFrame {
        size,
        prompt,
        overlay,
      };
      // Retain the painted prompt's cleanup state even if menu output fails.
      let result = renderer.update_completion(&mut frame, menu);
      renderer.rendered = Some(frame);
      result
    });
    if result.is_err() {
      app.invalidate_view();
    }
    result
  }

  /// Attempts to end and flush an update even after a begin or paint error.
  fn with_synchronized_update(
    &mut self,
    paint: impl FnOnce(&mut Self) -> io::Result<()>,
  ) -> io::Result<()> {
    let result =
      queue!(self.stdout, BeginSynchronizedUpdate).and_then(|()| paint(self));
    let end = queue!(self.stdout, EndSynchronizedUpdate);
    let flush = self.stdout.flush();
    result.and(end).and(flush)
  }

  /// Clears the recorded frame and restores any logs it covered.
  pub(super) fn clear_prompt(&mut self, size: (u16, u16)) -> io::Result<()> {
    let Some(frame) = self.rendered.take() else {
      return Ok(());
    };
    let width = usize::from(size.0.max(2));
    let layout = frame.clear_layout(width);
    self.cursor_row = self
      .cursor_row
      .map(|row| row.saturating_sub(layout.lines_above_cursor));
    queue!(self.stdout, MoveToColumn(0))?;
    if layout.lines_above_cursor > 0 {
      queue!(
        self.stdout,
        MoveUp(u16::try_from(layout.lines_above_cursor).unwrap_or(u16::MAX))
      )?;
    }
    // Movement can be clamped after a height reduction. Save the actual top.
    queue!(self.stdout, SavePosition)?;
    for index in 0..layout.block_lines {
      queue!(self.stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
      if index + 1 < layout.block_lines {
        queue!(self.stdout, MoveDown(1))?;
      }
    }
    queue!(self.stdout, RestorePosition, MoveToColumn(0))?;
    if let Some(overlay) = frame.overlay
      && overlay.menu.placement == CompletionPlacement::Above
    {
      let height = rows_at_width(
        overlay.underlay.iter().map(|row| (row.width, row.wrapped)),
        usize::from(frame.size.0),
        width,
      );
      self.write_log_rows(overlay.underlay)?;
      self.advance_cursor(height, size.1);
    }
    Ok(())
  }

  /// Prints and retains the same rendered rows for subsequent restoration.
  fn write_log_entry(
    &mut self,
    entry: LogEntry,
    height: u16,
  ) -> io::Result<()> {
    queue!(self.stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    let rows = self.logs.push(entry);
    self.advance_cursor(rows.len(), height);
    self.write_log_rows(rows)
  }

  fn write_log_rows(&mut self, rows: Vec<LogRow>) -> io::Result<()> {
    for row in rows {
      queue!(self.stdout, SetAttribute(Attribute::Reset), Print(row.text))?;
      if !row.wrapped {
        queue!(self.stdout, Print("\r\n"))?;
      }
    }
    queue!(self.stdout, SetAttribute(Attribute::Reset), ResetColor)
  }

  fn prepare_completion(
    &self,
    app: &mut TerminalApp,
    prompt: &PromptView,
    size: (u16, u16),
  ) -> Option<CompletionMenu> {
    if prompt.input_kind == InputKind::Confirmation {
      return None;
    }
    let state = app.completion.as_mut()?;
    let (placement, height) =
      self.completion_layout(state.candidates().len(), size.1)?;
    Some(CompletionMenu::new(
      state,
      placement,
      height,
      usize::from(size.0),
    ))
  }

  fn update_completion(
    &mut self,
    frame: &mut RenderedFrame,
    menu: Option<CompletionMenu>,
  ) -> io::Result<()> {
    let Some(menu) = menu else {
      return Ok(());
    };
    let underlay = if frame.overlay.is_none()
      && menu.placement == CompletionPlacement::Above
    {
      self.logs.tail(menu.rows.len())
    } else {
      Vec::new()
    };
    self.render_completion(&menu, frame.prompt.cursor_column)?;
    if let Some(overlay) = frame.overlay.as_mut() {
      // The footprint is unchanged, so its saved rows remain valid.
      overlay.menu = menu;
    } else {
      frame.overlay = Some(CompletionOverlay { menu, underlay });
    }
    Ok(())
  }

  /// Paints a prepared menu without uncovering its saved background.
  fn render_completion(
    &mut self,
    menu: &CompletionMenu,
    cursor: usize,
  ) -> io::Result<()> {
    let distance = u16::try_from(menu.rows.len() + 1).unwrap_or(u16::MAX);
    queue!(self.stdout, MoveToColumn(0))?;
    match menu.placement {
      CompletionPlacement::Above => queue!(self.stdout, MoveUp(distance))?,
      CompletionPlacement::Below => queue!(self.stdout, MoveDown(1))?,
    }
    queue!(self.stdout, Clear(ClearType::CurrentLine))?;
    for (index, row) in menu.rows.iter().enumerate() {
      queue!(self.stdout, MoveDown(1), MoveToColumn(0))?;
      if index == menu.selected {
        queue!(self.stdout, SetAttribute(Attribute::Reverse))?;
      }
      queue!(
        self.stdout,
        Print(&row.text),
        SetAttribute(Attribute::Reset),
        Clear(ClearType::UntilNewLine)
      )?;
    }
    match menu.placement {
      CompletionPlacement::Above => queue!(self.stdout, MoveDown(1))?,
      CompletionPlacement::Below => queue!(self.stdout, MoveUp(distance))?,
    }
    queue!(
      self.stdout,
      MoveToColumn(u16::try_from(cursor).unwrap_or(u16::MAX))
    )
  }

  /// Chooses the largest useful space, preferring below when the list fits.
  fn completion_layout(
    &self,
    candidates: usize,
    height: u16,
  ) -> Option<(CompletionPlacement, usize)> {
    if height < PROMPT_BLOCK_LINES {
      return None;
    }
    let desired = MAX_COMPLETION_ROWS.min(candidates);
    // Only cover owned log rows above. Below, the renderer owns the status
    // line and blank space, independently of how many logs were retained.
    let above = self.logs.available_rows().min(
      self
        .cursor_row
        .map_or(usize::MAX, |row| row.saturating_sub(1)),
    );
    // Without a cursor report, only the taskbar row is guaranteed to fit
    // below the separator. A clear establishes an exact position at row 0.
    let below = self
      .cursor_row
      .map_or(1, |row| usize::from(height).saturating_sub(row + 2));
    let (placement, available) = if below >= desired || below >= above {
      (CompletionPlacement::Below, below)
    } else {
      (CompletionPlacement::Above, above)
    };
    let count = desired.min(available);
    (count > 0).then_some((placement, count))
  }

  /// Renders the prepared input and taskbar with their normal spacing.
  fn render_prompt(&mut self, prompt: &PromptView) -> io::Result<()> {
    queue!(
      self.stdout,
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      Print("\r\n")
    )?;
    self.render_input(prompt)?;
    queue!(
      self.stdout,
      Print("\r\n"),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      Print("\r\n"),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      SetAttribute(Attribute::Dim),
      Print(&prompt.taskbar),
      SetAttribute(Attribute::Reset),
      MoveUp(2),
      SetCursorStyle::DefaultUserShape,
      Show,
      MoveToColumn(u16::try_from(prompt.cursor_column).unwrap_or(u16::MAX))
    )
  }

  /// Overwrites input before erasing any stale trailing characters.
  fn render_input(&mut self, prompt: &PromptView) -> io::Result<()> {
    queue!(
      self.stdout,
      MoveToColumn(0),
      SetAttribute(Attribute::Bold),
      Print(">"),
      SetAttribute(Attribute::Reset),
      Print(" ")
    )?;
    match prompt.input_kind {
      InputKind::Normal => queue!(self.stdout, Print(&prompt.input))?,
      InputKind::Placeholder => queue!(
        self.stdout,
        SetAttribute(Attribute::Dim),
        Print(&prompt.input),
        SetAttribute(Attribute::Reset)
      )?,
      InputKind::Confirmation => queue!(
        self.stdout,
        SetForegroundColor(Color::Yellow),
        Print(&prompt.input),
        ResetColor
      )?,
    }
    queue!(self.stdout, Clear(ClearType::UntilNewLine))
  }
}

/// The displayed prompt and everything needed to erase or restore its frame.
struct RenderedFrame {
  size: (u16, u16),
  prompt: PromptView,
  overlay: Option<CompletionOverlay>,
}

impl RenderedFrame {
  fn can_repaint(
    &self,
    prompt: &PromptView,
    menu: Option<&CompletionMenu>,
  ) -> bool {
    self.prompt.input_kind != InputKind::Confirmation
      && prompt.input_kind != InputKind::Confirmation
      && self.prompt.taskbar == prompt.taskbar
      && self
        .overlay
        .as_ref()
        .zip(menu)
        .is_some_and(|(old, new)| old.menu.same_footprint(new))
  }

  fn clear_layout(&self, width: usize) -> ClearLayout {
    let mut layout = self.prompt.clear_layout(width);
    if let Some(overlay) = &self.overlay {
      let rows =
        rows_at_width(overlay.row_metrics(), usize::from(self.size.0), width);
      match overlay.menu.placement {
        CompletionPlacement::Above => {
          layout.lines_above_cursor += rows.saturating_sub(1);
          layout.block_lines += rows.saturating_sub(1);
        }
        CompletionPlacement::Below => {
          // The menu replaced the taskbar, so only its actual rows reflow.
          layout.block_lines =
            1 + terminal_rows(self.prompt.input_width, width) + rows;
        }
      }
    }
    layout
  }
}

struct CompletionOverlay {
  menu: CompletionMenu,
  underlay: Vec<LogRow>,
}

impl CompletionOverlay {
  fn row_metrics(&self) -> impl Iterator<Item = (usize, bool)> + '_ {
    let prefix = self.underlay.len().saturating_sub(self.menu.rows.len());
    self
      .underlay
      .iter()
      .take(prefix)
      .map(|row| (row.width, row.wrapped))
      .chain(std::iter::once((0, false)))
      .chain(self.menu.rows.iter().map(|row| (row.width, false)))
  }
}

#[cfg(test)]
mod tests {
  mod io;

  use crossterm::cursor::{MoveToColumn, MoveUp};
  use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
  use crossterm::style::{Attribute, SetAttribute};
  use crossterm::terminal::{Clear, ClearType};

  use super::{CompletionOverlay, CompletionPlacement, PromptRenderer};
  use crate::app::{LogEntry, TerminalApp};
  use crate::ocpp::OcppVersion;
  use crate::simulator::UiLogLevel;

  fn menu_overlay(renderer: &PromptRenderer<Vec<u8>>) -> &CompletionOverlay {
    renderer
      .rendered
      .as_ref()
      .unwrap()
      .overlay
      .as_ref()
      .unwrap()
  }

  fn log(index: usize) -> LogEntry {
    LogEntry {
      timestamp: "12:34:56".to_string(),
      level: UiLogLevel::Rx,
      message: format!("log {index}"),
    }
  }

  fn press(app: &mut TerminalApp, key: KeyCode) {
    app.handle_key_event(KeyEvent::new(key, KeyModifiers::NONE));
  }

  fn output(renderer: &mut PromptRenderer<Vec<u8>>) -> String {
    String::from_utf8(std::mem::take(&mut renderer.stdout)).unwrap()
  }

  fn fixture() -> (PromptRenderer<Vec<u8>>, TerminalApp) {
    let mut renderer = PromptRenderer::new(Vec::new());
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    for index in 0..30 {
      app.push_log_entry(log(index));
    }
    renderer.draw(&mut app, (80, 24)).unwrap();
    output(&mut renderer);
    (renderer, app)
  }

  #[test]
  fn completion_draws_eight_candidates_without_adding_lines() {
    let (mut renderer, mut app) = fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    let frame = output(&mut renderer);
    let selected = format!(
      "{}  status{}",
      SetAttribute(Attribute::Reverse),
      SetAttribute(Attribute::Reset)
    );
    assert!(frame.contains(&selected));
    assert!(frame.contains("  start"));
    assert!(!frame.contains("  stop"));
    // Only the existing four-row prompt emits newlines. The overlay uses
    // cursor movement to cover eight logs plus the prompt's blank spacer.
    assert_eq!(frame.matches("\r\n").count(), 3);
    assert_eq!(menu_overlay(&renderer).row_metrics().count(), 9);
  }

  #[test]
  fn cycling_never_reprints_the_covered_logs() {
    let (mut renderer, mut app) = fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    output(&mut renderer);
    let underlay = menu_overlay(&renderer).underlay.clone();
    let saved_rows = menu_overlay(&renderer).underlay.as_ptr();
    let saved_text = menu_overlay(&renderer).underlay[0].text.as_ptr();
    for key in [KeyCode::Tab, KeyCode::BackTab] {
      for _ in 0..60 {
        press(&mut app, key);
        renderer.draw(&mut app, (80, 24)).unwrap();
        let frame = output(&mut renderer);
        assert!(!frame.contains("log "), "covered logs were restored");
        assert!(!frame.contains("disconnected"), "taskbar was redrawn");
        assert!(!frame.contains(['\r', '\n']), "prompt layout was rebuilt");
        assert_eq!(menu_overlay(&renderer).underlay, underlay);
        assert_eq!(menu_overlay(&renderer).underlay.as_ptr(), saved_rows);
        assert_eq!(
          menu_overlay(&renderer).underlay[0].text.as_ptr(),
          saved_text
        );
      }
    }
  }

  #[test]
  fn changing_candidates_with_the_same_footprint_reuses_the_saved_rows() {
    let (mut renderer, mut app) = fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    output(&mut renderer);
    let saved_rows = menu_overlay(&renderer).underlay.as_ptr();
    // A batch of editing and Tab events can change the completion source
    // before another frame is drawn, without changing the occupied area.
    app.clear_completion();
    app.set_profile_completions(
      (0..12).map(|i| format!("profile-{i}")).collect(),
    );
    app.input = "connect ".to_string();
    app.cursor = app.input.len();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    let frame = output(&mut renderer);
    assert!(!frame.contains("log "));
    assert!(!frame.contains(['\r', '\n']));
    assert_eq!(menu_overlay(&renderer).underlay.as_ptr(), saved_rows);
  }

  #[test]
  fn changes_outside_the_displayed_view_do_not_redraw() {
    let mut renderer = PromptRenderer::new(Vec::new());
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.set_connection_target(
      Some(format!("{}old", "profile".repeat(10))),
      None,
    );
    renderer.draw(&mut app, (30, 24)).unwrap();
    output(&mut renderer);
    app.set_connection_target(
      Some(format!("{}new", "profile".repeat(10))),
      None,
    );
    renderer.draw(&mut app, (30, 24)).unwrap();
    assert_eq!(output(&mut renderer), "");
  }

  #[test]
  fn clearing_consumes_the_recorded_frame() {
    let (mut renderer, mut app) = fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    renderer.clear_prompt((80, 24)).unwrap();
    assert!(renderer.rendered.is_none());
    assert!(!renderer.needs_cursor_position((40, 12)));
    output(&mut renderer);
    renderer.clear_prompt((80, 24)).unwrap();
    assert_eq!(output(&mut renderer), "");
  }

  #[test]
  fn shorter_candidates_overwrite_before_erasing_the_trailing_text() {
    let (mut renderer, mut app) = fixture();
    app.input = "con".to_string();
    app.cursor = app.input.len();
    for _ in 0..3 {
      press(&mut app, KeyCode::Tab);
      renderer.draw(&mut app, (80, 24)).unwrap();
    }
    assert_eq!(app.input, "connector-status ");
    output(&mut renderer);
    press(&mut app, KeyCode::BackTab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    let frame = output(&mut renderer);
    let replacement = format!(" connect {}", Clear(ClearType::UntilNewLine));
    assert!(frame.contains(&replacement));
    // The blank separator is the only row that is cleared in full.
    assert_eq!(
      frame
        .matches(&Clear(ClearType::CurrentLine).to_string())
        .count(),
      1
    );
  }

  #[test]
  fn status_changes_still_redraw_the_taskbar_while_completing() {
    let (mut renderer, mut app) = fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    output(&mut renderer);
    app.set_connection_target(Some("demo".to_string()), None);
    renderer.draw(&mut app, (80, 24)).unwrap();
    let frame = output(&mut renderer);
    assert!(frame.contains("profile demo | disconnected"));
    assert!(renderer.rendered.as_ref().unwrap().overlay.is_some());
  }

  #[test]
  fn dismissing_completion_replays_the_original_log_bytes() {
    for level in [
      UiLogLevel::Info,
      UiLogLevel::Warn,
      UiLogLevel::Error,
      UiLogLevel::Tx,
      UiLogLevel::Rx,
    ] {
      let mut renderer = PromptRenderer::new(Vec::new());
      let mut app = TerminalApp::new(OcppVersion::V2_1);
      for index in 0..2 {
        app.push_log_entry(LogEntry {
          level,
          ..log(index)
        });
      }
      renderer.draw(&mut app, (80, 24)).unwrap();
      let original = output(&mut renderer);
      let rows = renderer.logs.tail(2);
      for row in &rows {
        assert!(original.contains(&row.text));
      }
      for _ in 0..3 {
        press(&mut app, KeyCode::Tab);
        renderer.draw(&mut app, (80, 24)).unwrap();
        output(&mut renderer);
      }
      press(&mut app, KeyCode::Esc);
      renderer.draw(&mut app, (80, 24)).unwrap();
      let restored = output(&mut renderer);
      assert!(renderer.rendered.as_ref().unwrap().overlay.is_none());
      for row in rows {
        assert!(restored.contains(&row.text));
      }
      renderer.draw(&mut app, (80, 24)).unwrap();
      assert_eq!(output(&mut renderer), "");
    }
  }

  #[test]
  fn incoming_logs_restore_old_rows_before_redrawing_completion() {
    let (mut renderer, mut app) = fixture();
    for _ in 0..3 {
      press(&mut app, KeyCode::Tab);
      renderer.draw(&mut app, (80, 24)).unwrap();
      output(&mut renderer);
    }
    for index in 30..60 {
      app.push_log_entry(log(index));
    }
    renderer.draw(&mut app, (80, 24)).unwrap();
    let frame = output(&mut renderer);
    assert!(frame.find("log 29").unwrap() < frame.find("log 30").unwrap());
    assert!(frame.find("log 59").unwrap() < frame.find("  status").unwrap());
    assert!(renderer.rendered.as_ref().unwrap().overlay.is_some());
    assert!(
      renderer
        .logs
        .tail(1)
        .last()
        .unwrap()
        .text
        .contains("log 59")
    );
  }

  #[test]
  fn small_terminals_and_cleared_logs_limit_the_overlay() {
    for height in [4, 6, 24] {
      let mut renderer = PromptRenderer::new(Vec::new());
      renderer.set_cursor_row(Some(0));
      let mut app = TerminalApp::new(OcppVersion::V2_1);
      app.push_log_entry(log(0));
      renderer.draw(&mut app, (80, height)).unwrap();
      press(&mut app, KeyCode::Tab);
      renderer.draw(&mut app, (80, height)).unwrap();
      let overlay = menu_overlay(&renderer);
      assert_eq!(overlay.menu.placement, CompletionPlacement::Below);
      assert!(
        renderer.cursor_row.unwrap() + overlay.row_metrics().count()
          < usize::from(height)
      );
      app.clear_logs();
      renderer.draw(&mut app, (80, height)).unwrap();
      let overlay = menu_overlay(&renderer);
      assert_eq!(overlay.menu.placement, CompletionPlacement::Below);
      assert_eq!(
        overlay.row_metrics().count(),
        usize::from(height - 3).min(8) + 1
      );
      assert_eq!(renderer.logs.available_rows(), 0);
    }
  }

  fn cleared_fixture() -> (PromptRenderer<Vec<u8>>, TerminalApp) {
    let (mut renderer, mut app) = fixture();
    app.clear_logs();
    renderer.draw(&mut app, (80, 24)).unwrap();
    output(&mut renderer);
    assert_eq!(renderer.cursor_row, Some(1));
    (renderer, app)
  }

  #[test]
  fn completion_after_clear_uses_space_below_and_restores_the_taskbar() {
    let (mut renderer, mut app) = cleared_fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    let frame = output(&mut renderer);
    let overlay = menu_overlay(&renderer);
    assert_eq!(overlay.menu.placement, CompletionPlacement::Below);
    assert_eq!(overlay.row_metrics().count(), 9);
    assert_eq!(overlay.underlay.len(), 0);
    assert!(frame.contains(&format!("{}{}", MoveUp(9), MoveToColumn(9))));
    assert_eq!(renderer.cursor_row, Some(1));

    press(&mut app, KeyCode::Esc);
    renderer.draw(&mut app, (80, 24)).unwrap();
    let frame = output(&mut renderer);
    assert!(renderer.rendered.as_ref().unwrap().overlay.is_none());
    assert!(frame.contains("url - | disconnected"));
    assert!(!frame.contains("  status"));
    assert!(!frame.contains("12:34:56"));
    // Eleven rows cover the prompt and old menu; three more clears belong
    // to the normal prompt redraw. No candidate rows are left underneath.
    assert_eq!(
      frame
        .matches(&Clear(ClearType::CurrentLine).to_string())
        .count(),
      14
    );
    assert_eq!(renderer.cursor_row, Some(1));
  }

  #[test]
  fn cycling_below_does_not_reveal_the_taskbar_or_move_the_input() {
    let (mut renderer, mut app) = cleared_fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    output(&mut renderer);
    for key in [KeyCode::Tab, KeyCode::BackTab] {
      for _ in 0..60 {
        press(&mut app, key);
        renderer.draw(&mut app, (80, 24)).unwrap();
        let frame = output(&mut renderer);
        assert!(!frame.contains("disconnected"));
        assert!(!frame.contains(['\r', '\n']));
        assert_eq!(renderer.cursor_row, Some(1));
        assert_eq!(
          menu_overlay(&renderer).menu.placement,
          CompletionPlacement::Below
        );
      }
    }
  }

  #[test]
  fn incoming_logs_switch_above_only_when_the_list_no_longer_fits_below() {
    let (mut renderer, mut app) = cleared_fixture();
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    output(&mut renderer);
    for index in 1..=15 {
      app.push_log_entry(log(index));
      renderer.draw(&mut app, (80, 24)).unwrap();
      let frame = output(&mut renderer);
      assert!(frame.contains(&format!("log {index}")));
      assert_eq!(renderer.cursor_row, Some(index + 1));
      let expected = if index < 14 {
        CompletionPlacement::Below
      } else {
        CompletionPlacement::Above
      };
      assert_eq!(menu_overlay(&renderer).menu.placement, expected);
    }
    press(&mut app, KeyCode::Esc);
    renderer.draw(&mut app, (80, 24)).unwrap();
    assert_eq!(renderer.cursor_row, Some(16));
    assert!(output(&mut renderer).contains("log 15"));
  }

  #[test]
  fn placement_prefers_below_when_it_fits_and_uses_only_owned_rows() {
    let mut renderer = PromptRenderer::new(Vec::new());
    for index in 0..5 {
      renderer.logs.push(log(index));
    }
    renderer.set_cursor_row(Some(6));
    assert_eq!(
      renderer.completion_layout(3, 24),
      Some((CompletionPlacement::Below, 3))
    );
    assert_eq!(
      renderer.completion_layout(20, 24),
      Some((CompletionPlacement::Below, 8))
    );
    // Ties also prefer below when neither side can fit the full list.
    renderer.set_cursor_row(Some(17));
    assert_eq!(
      renderer.completion_layout(20, 24),
      Some((CompletionPlacement::Below, 5))
    );
    // Older shell output above the five owned logs cannot be restored.
    renderer.set_cursor_row(Some(21));
    assert_eq!(
      renderer.completion_layout(20, 24),
      Some((CompletionPlacement::Above, 5))
    );
    for index in 5..10 {
      renderer.logs.push(log(index));
    }
    // Eight rows below fit exactly; one fewer requires the above fallback.
    renderer.set_cursor_row(Some(14));
    assert_eq!(
      renderer.completion_layout(20, 24),
      Some((CompletionPlacement::Below, 8))
    );
    renderer.set_cursor_row(Some(15));
    assert_eq!(
      renderer.completion_layout(20, 24),
      Some((CompletionPlacement::Above, 8))
    );
    renderer.logs.clear();
    renderer.set_cursor_row(None);
    assert_eq!(
      renderer.completion_layout(20, 24),
      Some((CompletionPlacement::Below, 1))
    );
    assert_eq!(renderer.completion_layout(20, 3), None);
  }

  #[test]
  fn resizing_a_below_menu_uses_its_height_instead_of_the_hidden_taskbar() {
    let (mut renderer, mut app) = cleared_fixture();
    app.set_connection_target(Some("profile".repeat(20)), None);
    app.input = "sta".to_string();
    app.cursor = 3;
    press(&mut app, KeyCode::Tab);
    renderer.draw(&mut app, (80, 24)).unwrap();
    assert!(!renderer.needs_cursor_position((80, 24)));
    assert!(renderer.needs_cursor_position((20, 24)));
    output(&mut renderer);
    // The short input still occupies one row after terminal reflow.
    renderer.set_cursor_row(Some(1));
    renderer.clear_prompt((20, 24)).unwrap();
    let frame = output(&mut renderer);
    // Three candidates plus their separator occupy four rows below input.
    // The long status text was covered, so its potential wraps do not count.
    assert_eq!(
      frame
        .matches(&Clear(ClearType::CurrentLine).to_string())
        .count(),
      6
    );
    assert_eq!(renderer.cursor_row, Some(0));
    renderer.draw(&mut app, (20, 24)).unwrap();
    assert_eq!(renderer.cursor_row, Some(1));
    assert_eq!(
      menu_overlay(&renderer).menu.placement,
      CompletionPlacement::Below
    );
  }
}
