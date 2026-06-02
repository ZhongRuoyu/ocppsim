use std::collections::VecDeque;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{self, BufWriter, Stdout, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;
use crossterm::event::{
  self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
  KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
  EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{
  Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
  Wrap,
};
use ratatui::{Frame, Terminal};

use crate::ocpp::OcppVersion;
use crate::simulator::{
  ConnectorSnapshot, SimulatorSnapshot, UiEvent, UiLogLevel,
};

mod completion;
mod history;

use completion::{CompletionState, completion_seed};
use history::CommandHistory;

const MAX_LOG_LINES: usize = 10_000;
const SCROLL_WHEEL_STEP: usize = 1;

pub struct TerminalSession {
  terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
  /// Initializes terminal raw mode and enters the alternate screen.
  pub fn new() -> Result<Self> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(Self { terminal })
  }

  /// Redraws the full terminal UI using the current app state.
  pub fn draw(&mut self, app: &mut TerminalApp) -> Result<()> {
    self.terminal.draw(|frame| app.render(frame))?;
    Ok(())
  }

  /// Polls keyboard and mouse input and maps it to an app action.
  ///
  /// Returns [`InputAction::None`] when no event is available.
  pub fn poll_input(app: &mut TerminalApp) -> Result<InputAction> {
    // Wait up to 50ms for the first available event.
    if !event::poll(Duration::from_millis(50))? {
      return Ok(InputAction::None);
    }

    // Drain all pending events. Return immediately on any non-trivial action
    // so the caller can handle it before the next redraw.
    loop {
      let action = match event::read()? {
        Event::Key(key) => {
          if key.kind == KeyEventKind::Press {
            app.handle_key_event(key)
          } else {
            InputAction::None
          }
        }
        Event::Mouse(mouse) => app.handle_mouse_event(mouse),
        _ => InputAction::None,
      };
      if !matches!(action, InputAction::None) {
        return Ok(action);
      }
      if !event::poll(Duration::ZERO)? {
        break;
      }
    }
    Ok(InputAction::None)
  }
}

impl Drop for TerminalSession {
  /// Restores terminal state when the session leaves scope.
  fn drop(&mut self) {
    restore_console();
    let _ = self.terminal.show_cursor();
  }
}

/// Restores console modes and leaves alternate screen mode.
pub fn restore_console() {
  let _ = disable_raw_mode();
  let mut stdout = io::stdout();
  let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
}

#[derive(Debug, Clone)]
struct LogEntry {
  timestamp: String,
  level: UiLogLevel,
  message: String,
  formatted: String,
}

#[derive(Debug)]
struct FileLogSink {
  path: PathBuf,
  writer: BufWriter<File>,
}

impl FileLogSink {
  /// Opens a file sink in append mode, creating parent directories as needed.
  fn open(path: &Path) -> Result<Self> {
    if let Some(parent) = path.parent()
      && !parent.as_os_str().is_empty()
    {
      create_dir_all(parent).with_context(|| {
        format!("Failed to create log directory {}", parent.display())
      })?;
    }

    let file = OpenOptions::new()
      .create(true)
      .append(true)
      .open(path)
      .with_context(|| format!("Failed to open log file {}", path.display()))?;

    Ok(Self {
      path: path.to_path_buf(),
      writer: BufWriter::new(file),
    })
  }

  /// Appends one formatted log line and flushes the writer.
  fn append_line(
    &mut self,
    timestamp: &str,
    level: UiLogLevel,
    message: &str,
  ) -> io::Result<()> {
    writeln!(self.writer, "{} [{}] {}", timestamp, level.label(), message)?;
    self.writer.flush()
  }
}

pub enum InputAction {
  None,
  Submitted(String),
  ExitRequested,
}

pub struct TerminalApp {
  protocol: OcppVersion,
  ws_url: String,
  logs: VecDeque<LogEntry>,
  wrapped_log_line_counts: VecDeque<usize>,
  total_wrapped_log_lines: usize,
  wrapped_line_count_width: usize,
  input: String,
  cursor: usize,
  log_scroll: usize,
  follow_logs: bool,
  log_view_height: usize,
  log_view_width: usize,
  history: CommandHistory,
  known_connectors: Vec<u16>,
  completion: Option<CompletionState>,
  log_sink: Option<FileLogSink>,
}

impl TerminalApp {
  /// Creates a fresh terminal app state for the selected protocol.
  pub fn new(protocol: OcppVersion) -> Self {
    Self {
      protocol,
      ws_url: String::new(),
      logs: VecDeque::new(),
      wrapped_log_line_counts: VecDeque::new(),
      total_wrapped_log_lines: 0,
      wrapped_line_count_width: 1,
      input: String::new(),
      cursor: 0,
      log_scroll: 0,
      follow_logs: true,
      log_view_height: 1,
      log_view_width: 1,
      history: CommandHistory::new(),
      known_connectors: Vec::new(),
      completion: None,
      log_sink: None,
    }
  }

  /// Enables persistent log appending to `path`.
  pub fn enable_log_path(&mut self, path: &Path) -> Result<()> {
    self.log_sink = Some(FileLogSink::open(path)?);
    Ok(())
  }

  /// Applies one simulator UI event to the terminal state.
  pub fn apply(&mut self, event: UiEvent) {
    match event {
      UiEvent::Log { level, message } => self.push_log(level, message),
      UiEvent::Snapshot(snapshot) => self.push_snapshot(snapshot),
    }
  }

  /// Adds an informational log entry.
  pub fn push_info<S: Into<String>>(&mut self, message: S) {
    self.push_log(UiLogLevel::Info, message);
  }

  /// Adds an error log entry.
  pub fn push_error<S: Into<String>>(&mut self, message: S) {
    self.push_log(UiLogLevel::Error, message);
  }

  /// Logs a submitted command and stores it in command history.
  pub fn push_user_input(&mut self, message: &str) {
    self.history.record(message);
    self.push_log(UiLogLevel::Info, format!("> {message}"));
  }

  /// Clears visible logs and resets scrolling to follow mode.
  pub fn clear_logs(&mut self) {
    self.logs.clear();
    self.wrapped_log_line_counts.clear();
    self.total_wrapped_log_lines = 0;
    self.log_scroll = 0;
    self.follow_logs = true;
  }

  /// Handles mouse input, currently used for scroll-wheel log navigation.
  pub fn handle_mouse_event(&mut self, event: MouseEvent) -> InputAction {
    self.clear_completion();
    match event.kind {
      MouseEventKind::ScrollUp => {
        self.scroll_logs_up(SCROLL_WHEEL_STEP);
      }
      MouseEventKind::ScrollDown => {
        self.scroll_logs_down(SCROLL_WHEEL_STEP);
      }
      _ => {}
    }
    InputAction::None
  }

  /// Handles keyboard input for editing, history, completion, and submit.
  pub fn handle_key_event(&mut self, key: KeyEvent) -> InputAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
      return self.handle_ctrl_key(key);
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
      return self.handle_alt_key(key);
    }

    self.handle_plain_key_event(key)
  }

  fn handle_plain_key_event(&mut self, key: KeyEvent) -> InputAction {
    match key.code {
      KeyCode::BackTab => {
        self.leave_history_navigation();
        self.complete_input(true);
        InputAction::None
      }
      KeyCode::Tab => {
        self.leave_history_navigation();
        self.complete_input(key.modifiers.contains(KeyModifiers::SHIFT));
        InputAction::None
      }
      KeyCode::Up => {
        self.clear_completion();
        self.select_previous_history();
        InputAction::None
      }
      KeyCode::Down => {
        self.clear_completion();
        self.select_next_history();
        InputAction::None
      }
      KeyCode::PageUp => {
        self.clear_completion();
        self.scroll_logs_full_page_up();
        InputAction::None
      }
      KeyCode::PageDown => {
        self.clear_completion();
        self.scroll_logs_full_page_down();
        InputAction::None
      }
      KeyCode::Char(ch) => self.handle_char_key_event(ch, key.modifiers),
      KeyCode::Backspace => {
        self.clear_completion();
        self.leave_history_navigation();
        if self.cursor > 0 {
          let previous = previous_char_boundary(&self.input, self.cursor);
          self.input.remove(previous);
          self.cursor = previous;
        }
        InputAction::None
      }
      KeyCode::Delete => {
        self.clear_completion();
        self.leave_history_navigation();
        if self.cursor < self.input.len() {
          self.input.remove(self.cursor);
        }
        InputAction::None
      }
      KeyCode::Left => {
        self.clear_completion();
        if self.cursor > 0 {
          self.cursor = previous_char_boundary(&self.input, self.cursor);
        }
        InputAction::None
      }
      KeyCode::Right => {
        self.clear_completion();
        if self.cursor < self.input.len() {
          self.cursor = next_char_boundary(&self.input, self.cursor);
        }
        InputAction::None
      }
      KeyCode::Home => {
        self.clear_completion();
        self.cursor = 0;
        InputAction::None
      }
      KeyCode::End => {
        self.clear_completion();
        self.cursor = self.input.len();
        InputAction::None
      }
      KeyCode::Esc => {
        self.clear_completion();
        self.leave_history_navigation();
        self.input.clear();
        self.cursor = 0;
        InputAction::None
      }
      KeyCode::Enter => {
        self.clear_completion();
        self.leave_history_navigation();
        let line = self.input.trim().to_string();
        self.input.clear();
        self.cursor = 0;
        InputAction::Submitted(line)
      }
      _ => InputAction::None,
    }
  }

  fn handle_char_key_event(
    &mut self,
    ch: char,
    modifiers: KeyModifiers,
  ) -> InputAction {
    if modifiers != KeyModifiers::NONE && modifiers != KeyModifiers::SHIFT {
      return InputAction::None;
    }
    self.clear_completion();
    self.leave_history_navigation();
    self.input.insert(self.cursor, ch);
    self.cursor += ch.len_utf8();
    InputAction::None
  }

  /// Renders the log pane and command input pane for the current frame.
  pub fn render(&mut self, frame: &mut Frame<'_>) {
    let areas = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Min(5), Constraint::Length(3)])
      .split(frame.area());

    self.log_view_height = areas[0].height.saturating_sub(2) as usize;
    self.log_view_height = self.log_view_height.max(1);
    self.log_view_width = areas[0].width.saturating_sub(2) as usize;
    self.log_view_width = self.log_view_width.max(1);
    let max_scroll = self.max_log_scroll();
    if self.follow_logs || self.log_scroll > max_scroll {
      self.log_scroll = max_scroll;
    }

    let mut scrollbar_state = ScrollbarState::new(max_scroll)
      .viewport_content_length(self.log_view_height)
      .position(self.log_scroll);

    let title = if self.ws_url.is_empty() {
      format!("Logs - OCPP {}", self.protocol.label())
    } else {
      format!("Logs - OCPP {} - {}", self.protocol.label(), self.ws_url)
    };
    let logs_block = Block::default().title(title).borders(Borders::ALL);
    let lines = self.render_log_lines();
    let logs = Paragraph::new(lines)
      .block(logs_block)
      .wrap(Wrap { trim: false })
      .scroll((u16::try_from(self.log_scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(logs, areas[0]);
    frame.render_stateful_widget(
      Scrollbar::new(ScrollbarOrientation::VerticalRight),
      areas[0],
      &mut scrollbar_state,
    );

    let input_block = Block::default().title("Command").borders(Borders::ALL);
    let input = Paragraph::new(self.input.as_str()).block(input_block);
    frame.render_widget(input, areas[1]);

    let max_cursor = areas[1].width.saturating_sub(3) as usize;
    let cursor = self.cursor_display_width().min(max_cursor);
    frame.set_cursor_position((
      areas[1].x + 1 + u16::try_from(cursor).unwrap_or(0),
      areas[1].y + 1,
    ));
  }

  /// Handles Ctrl-modified editing and exit shortcuts.
  fn handle_ctrl_key(&mut self, key: KeyEvent) -> InputAction {
    match key.code {
      KeyCode::Char('c' | 'd') => InputAction::ExitRequested,
      KeyCode::Char('w') => {
        self.clear_completion();
        self.leave_history_navigation();
        self.clear_previous_word();
        InputAction::None
      }
      KeyCode::Char('u') => {
        self.clear_completion();
        self.leave_history_navigation();
        self.input.clear();
        self.cursor = 0;
        InputAction::None
      }
      KeyCode::Char('a') => {
        self.clear_completion();
        self.cursor = 0;
        InputAction::None
      }
      KeyCode::Char('e') => {
        self.clear_completion();
        self.cursor = self.input.len();
        InputAction::None
      }
      _ => InputAction::None,
    }
  }

  /// Handles Alt-modified shortcuts for log navigation and word movement.
  fn handle_alt_key(&mut self, key: KeyEvent) -> InputAction {
    self.clear_completion();
    match key.code {
      KeyCode::Up => {
        self.scroll_logs_half_page_up();
      }
      KeyCode::Down => {
        self.scroll_logs_half_page_down();
      }
      KeyCode::Left | KeyCode::Char('b') => {
        self.move_cursor_word_left();
      }
      KeyCode::Right | KeyCode::Char('f') => {
        self.move_cursor_word_right();
      }
      _ => {}
    }
    InputAction::None
  }

  /// Moves backward through command history and updates the input buffer.
  fn select_previous_history(&mut self) {
    if let Some(input) = self.history.previous(&self.input) {
      self.input = input;
      self.cursor = self.input.len();
    }
  }

  /// Moves forward through command history or restores the draft input.
  fn select_next_history(&mut self) {
    if let Some(input) = self.history.next() {
      self.input = input;
      self.cursor = self.input.len();
    }
  }

  /// Exits history navigation mode and clears the draft snapshot.
  fn leave_history_navigation(&mut self) {
    self.history.leave_navigation();
  }

  /// Performs tab completion cycling for the current input line.
  ///
  /// When `reverse` is `true`, candidates are traversed backward.
  fn complete_input(&mut self, reverse: bool) {
    if self.cursor != self.input.len() {
      self.cursor = self.input.len();
    }

    if let Some(state) = self.completion.as_mut() {
      if state.is_empty() {
        self.completion = None;
        return;
      }
      self.input = state.next_value(reverse);
      self.cursor = self.input.len();
      return;
    }

    let Some((base, candidates)) =
      completion_seed(&self.input, self.protocol, &self.known_connectors)
    else {
      return;
    };
    if candidates.is_empty() {
      return;
    }

    let index = if reverse { candidates.len() - 1 } else { 0 };

    self.input = format!("{}{}", base, candidates[index]);
    self.cursor = self.input.len();

    if candidates.len() > 1 {
      self.completion = Some(CompletionState::new(base, candidates, index));
    }
  }

  /// Deletes the word directly before the current cursor position.
  fn clear_previous_word(&mut self) {
    if self.cursor == 0 {
      return;
    }

    let mut start = self.cursor;

    while start > 0 {
      let previous = previous_char_boundary(&self.input, start);
      let ch = self.input[previous..start]
        .chars()
        .next()
        .expect("char boundary should contain one character");
      if !ch.is_whitespace() {
        break;
      }
      start = previous;
    }
    while start > 0 {
      let previous = previous_char_boundary(&self.input, start);
      let ch = self.input[previous..start]
        .chars()
        .next()
        .expect("char boundary should contain one character");
      if ch.is_whitespace() {
        break;
      }
      start = previous;
    }

    self.input.drain(start..self.cursor);
    self.cursor = start;
  }

  /// Moves the cursor one word to the left.
  fn move_cursor_word_left(&mut self) {
    let mut pos = self.cursor;
    while pos > 0 {
      let previous = previous_char_boundary(&self.input, pos);
      let ch = self.input[previous..pos]
        .chars()
        .next()
        .expect("char boundary should contain one character");
      if !ch.is_whitespace() {
        break;
      }
      pos = previous;
    }
    while pos > 0 {
      let previous = previous_char_boundary(&self.input, pos);
      let ch = self.input[previous..pos]
        .chars()
        .next()
        .expect("char boundary should contain one character");
      if ch.is_whitespace() {
        break;
      }
      pos = previous;
    }
    self.cursor = pos;
  }

  /// Moves the cursor one word to the right.
  fn move_cursor_word_right(&mut self) {
    let len = self.input.len();
    let mut pos = self.cursor;
    while pos < len {
      let next = next_char_boundary(&self.input, pos);
      let ch = self.input[pos..next]
        .chars()
        .next()
        .expect("char boundary should contain one character");
      if !ch.is_whitespace() {
        break;
      }
      pos = next;
    }
    while pos < len {
      let next = next_char_boundary(&self.input, pos);
      let ch = self.input[pos..next]
        .chars()
        .next()
        .expect("char boundary should contain one character");
      if ch.is_whitespace() {
        break;
      }
      pos = next;
    }
    self.cursor = pos;
  }

  /// Returns the displayed width before the byte-index cursor.
  fn cursor_display_width(&self) -> usize {
    Line::raw(self.input[..self.cursor].to_string()).width()
  }

  /// Clears any active tab-completion state.
  fn clear_completion(&mut self) {
    self.completion = None;
  }

  /// Scrolls logs up by a full page.
  fn scroll_logs_full_page_up(&mut self) {
    self.scroll_logs_up(self.log_view_height);
  }

  /// Scrolls logs down by a full page.
  fn scroll_logs_full_page_down(&mut self) {
    self.scroll_logs_down(self.log_view_height);
  }

  /// Scrolls logs up by half of the currently visible log pane.
  fn scroll_logs_half_page_up(&mut self) {
    let lines = (self.log_view_height / 2).max(1);
    self.scroll_logs_up(lines);
  }

  /// Scrolls logs down by half of the currently visible log pane.
  fn scroll_logs_half_page_down(&mut self) {
    let lines = (self.log_view_height / 2).max(1);
    self.scroll_logs_down(lines);
  }

  /// Scrolls logs upward by `lines` wrapped display rows.
  fn scroll_logs_up(&mut self, lines: usize) {
    if lines == 0 {
      return;
    }
    let current = if self.follow_logs {
      self.max_log_scroll()
    } else {
      self.log_scroll
    };
    self.log_scroll = current.saturating_sub(lines);
    self.follow_logs = false;
  }

  /// Scrolls logs downward by `lines` wrapped display rows.
  fn scroll_logs_down(&mut self, lines: usize) {
    if lines == 0 {
      return;
    }
    let max_scroll = self.max_log_scroll();
    let current = if self.follow_logs {
      max_scroll
    } else {
      self.log_scroll
    };
    self.log_scroll = current.saturating_add(lines).min(max_scroll);
    self.follow_logs = self.log_scroll == max_scroll;
  }

  /// Returns the maximum vertical scroll offset for wrapped log content.
  fn max_log_scroll(&mut self) -> usize {
    self.refresh_wrapped_log_line_counts();
    self
      .total_wrapped_log_lines
      .saturating_sub(self.log_view_height)
  }

  /// Renders all log entries into styled terminal lines.
  fn render_log_lines(&self) -> Vec<Line<'_>> {
    self
      .logs
      .iter()
      .map(|entry| {
        Line::styled(
          entry.formatted.as_str(),
          Style::default().fg(entry.level.color()),
        )
      })
      .collect()
  }

  /// Formats one log entry using the configured timestamp and level pattern.
  fn format_log_entry(entry: &LogEntry) -> String {
    format!(
      "{} [{}] {}",
      entry.timestamp,
      entry.level.label(),
      entry.message
    )
  }

  /// Pushes one or more log lines to memory and optional file sink.
  ///
  /// Multi-line messages are split so each line gets its own timestamped
  /// entry. On file write failure, file logging is disabled and an error line
  /// is appended to in-memory logs.
  fn push_log<S: Into<String>>(&mut self, level: UiLogLevel, message: S) {
    let message = message.into();
    let mut sink_error: Option<String> = None;

    for line in message.lines() {
      let timestamp = log_timestamp_now();
      self.push_log_entry(LogEntry {
        timestamp: timestamp.clone(),
        level,
        message: line.to_string(),
        formatted: String::new(),
      });

      if let Some(sink) = self.log_sink.as_mut()
        && let Err(error) = sink.append_line(&timestamp, level, line)
        && sink_error.is_none()
      {
        sink_error = Some(format!(
          "Failed to append logs to {}: {}",
          sink.path.display(),
          error
        ));
      }
    }

    if let Some(error) = sink_error {
      self.log_sink = None;
      self.push_log(UiLogLevel::Error, error);
    }

    let mut removed_wrapped_lines: usize = 0;
    while self.logs.len() > MAX_LOG_LINES {
      let _ = self.logs.pop_front();
      if let Some(wrapped_lines) = self.wrapped_log_line_counts.pop_front() {
        removed_wrapped_lines =
          removed_wrapped_lines.saturating_add(wrapped_lines);
      }
    }
    self.total_wrapped_log_lines = self
      .total_wrapped_log_lines
      .saturating_sub(removed_wrapped_lines);

    if removed_wrapped_lines > 0 && !self.follow_logs {
      self.log_scroll = self.log_scroll.saturating_sub(removed_wrapped_lines);
    }
  }

  /// Appends one in-memory log entry and updates wrapped-line accounting.
  fn push_log_entry(&mut self, mut entry: LogEntry) {
    entry.formatted = Self::format_log_entry(&entry);
    let width = self.wrapped_line_count_width.max(1);
    let wrapped_lines = Self::wrapped_log_line_count(&entry, width);
    self.total_wrapped_log_lines =
      self.total_wrapped_log_lines.saturating_add(wrapped_lines);
    self.wrapped_log_line_counts.push_back(wrapped_lines);
    self.logs.push_back(entry);
  }

  /// Rebuilds wrapped-line counts when view width or cache shape changes.
  fn refresh_wrapped_log_line_counts(&mut self) {
    let width = self.log_view_width.max(1);
    if width == self.wrapped_line_count_width
      && self.wrapped_log_line_counts.len() == self.logs.len()
    {
      return;
    }

    self.wrapped_line_count_width = width;
    self.wrapped_log_line_counts.clear();
    self.total_wrapped_log_lines = 0;
    for entry in &self.logs {
      let wrapped_lines = Self::wrapped_log_line_count(entry, width);
      self.total_wrapped_log_lines =
        self.total_wrapped_log_lines.saturating_add(wrapped_lines);
      self.wrapped_log_line_counts.push_back(wrapped_lines);
    }
  }

  /// Returns wrapped terminal rows occupied by one entry at `width`.
  fn wrapped_log_line_count(entry: &LogEntry, width: usize) -> usize {
    let width = u16::try_from(width.max(1)).unwrap_or(u16::MAX);
    Paragraph::new(Line::raw(entry.formatted.as_str()))
      .wrap(Wrap { trim: false })
      .line_count(width)
      .max(1)
  }

  /// Applies a simulator snapshot by logging summary and connector details.
  fn push_snapshot(&mut self, snapshot: SimulatorSnapshot) {
    self.known_connectors =
      snapshot.connectors.iter().map(|item| item.id).collect();
    self.ws_url.clone_from(&snapshot.connection_url);

    self.push_log(
      UiLogLevel::Info,
      format!(
        "CP {} protocol={} connected={} heartbeat={} queue={} pending={}",
        snapshot.cp_id,
        snapshot.protocol,
        snapshot.connected,
        display_heartbeat(snapshot.heartbeat_seconds),
        snapshot.queue_depth,
        snapshot.pending_action.unwrap_or_else(|| "-".to_string()),
      ),
    );

    for connector in snapshot.connectors {
      self.push_log(UiLogLevel::Info, format_connector_line(connector));
    }
  }
}

/// Formats one connector snapshot line for UI logging.
fn format_connector_line(connector: ConnectorSnapshot) -> String {
  let tx = connector.transaction.unwrap_or_else(|| "-".to_string());
  format!(
    "Connector {} status={} meter={}Wh tx={}",
    connector.id, connector.status, connector.meter_wh, tx,
  )
}

/// Formats heartbeat interval for display.
fn display_heartbeat(value: Option<u64>) -> String {
  value.map_or_else(|| "-".to_string(), |seconds| format!("{seconds}s"))
}

/// Returns the local timestamp string used in log entries.
fn log_timestamp_now() -> String {
  Local::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

/// Returns the byte index of the previous UTF-8 character boundary.
fn previous_char_boundary(input: &str, index: usize) -> usize {
  input[..index]
    .char_indices()
    .next_back()
    .map_or(0, |(position, _)| position)
}

/// Returns the byte index of the next UTF-8 character boundary.
fn next_char_boundary(input: &str, index: usize) -> usize {
  input[index..]
    .char_indices()
    .nth(1)
    .map_or(input.len(), |(position, _)| index + position)
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::time::{SystemTime, UNIX_EPOCH};

  use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

  use crate::ocpp::OcppVersion;

  use super::{InputAction, LogEntry, TerminalApp};
  use crate::simulator::UiLogLevel;

  static TEMP_LOG_COUNTER: AtomicU64 = AtomicU64::new(0);

  #[test]
  /// Verifies UI log entries are appended to configured log file sink.
  fn appends_logs_to_file() {
    let path = temp_log_path();
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.enable_log_path(&path).expect("log file should open");

    app.push_info("hello");
    app.push_error("oops");

    let content = std::fs::read_to_string(&path).expect("log file should read");
    let first_line = content.lines().next().expect("first log line");
    assert!(first_line.contains(" [INFO] hello"));
    assert!(first_line.len() >= 20);
    assert_eq!(first_line.chars().nth(4), Some('-'));
    assert_eq!(first_line.chars().nth(7), Some('-'));
    assert_eq!(first_line.chars().nth(10), Some(' '));
    assert_eq!(first_line.chars().nth(13), Some(':'));
    assert_eq!(first_line.chars().nth(16), Some(':'));
    assert!(content.contains("[ERROR] oops"));

    let _ = std::fs::remove_file(path);
  }

  #[test]
  /// Verifies log scroll bounds follow paragraph word-wrapping behavior.
  fn max_log_scroll_accounts_for_word_wrapping() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.log_view_height = 1;
    app.log_view_width = 10;
    app.push_log_entry(LogEntry {
      timestamp: "t".to_string(),
      level: UiLogLevel::Tx,
      message: "123456789012".to_string(),
      formatted: String::new(),
    });
    assert_eq!(app.max_log_scroll(), 2);
  }

  #[test]
  /// Verifies wrapped-line cache refreshes when the view width changes.
  fn max_log_scroll_recomputes_after_width_change() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.log_view_height = 1;
    app.log_view_width = 30;
    app.push_log_entry(LogEntry {
      timestamp: "t".to_string(),
      level: UiLogLevel::Tx,
      message: "word word word word word word".to_string(),
      formatted: String::new(),
    });

    let wide_scroll = app.max_log_scroll();
    app.log_view_width = 10;
    let narrow_scroll = app.max_log_scroll();
    assert!(narrow_scroll > wide_scroll);
  }

  #[test]
  /// Verifies non-ASCII insertion and deletion keep byte indices valid.
  fn edits_non_ascii_input_on_character_boundaries() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);

    press(&mut app, KeyCode::Char('é'));
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.input, "éx");
    assert_eq!(app.cursor, app.input.len());
    assert_eq!(app.cursor_display_width(), 2);

    press(&mut app, KeyCode::Left);
    assert_eq!(app.cursor, "é".len());
    press(&mut app, KeyCode::Backspace);
    assert_eq!(app.input, "x");
    assert_eq!(app.cursor, 0);

    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Char('ø'));
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Delete);
    assert_eq!(app.input, "x");
    assert_eq!(app.cursor, app.input.len());
  }

  #[test]
  /// Verifies word deletion treats UTF-8 text as character boundaries.
  fn clears_non_ascii_words_without_panicking() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);

    for ch in "start café now".chars() {
      press(&mut app, KeyCode::Char(ch));
    }

    press_with_modifiers(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    assert_eq!(app.input, "start café ");
    assert_eq!(app.cursor, app.input.len());

    press_with_modifiers(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    assert_eq!(app.input, "start ");
    assert_eq!(app.cursor, app.input.len());

    press_with_modifiers(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    assert_eq!(app.input, "");
    assert_eq!(app.cursor, 0);
  }

  #[test]
  /// Verifies Alt+Left navigates one word to the left.
  fn alt_left_moves_cursor_one_word_left() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    for ch in "start 1 local".chars() {
      press(&mut app, KeyCode::Char(ch));
    }
    assert_eq!(app.cursor, app.input.len());

    press_with_modifiers(&mut app, KeyCode::Left, KeyModifiers::ALT);
    assert_eq!(app.cursor, "start 1 ".len());

    press_with_modifiers(&mut app, KeyCode::Left, KeyModifiers::ALT);
    assert_eq!(app.cursor, "start ".len());

    press_with_modifiers(&mut app, KeyCode::Left, KeyModifiers::ALT);
    assert_eq!(app.cursor, 0);

    // Should not go below zero.
    press_with_modifiers(&mut app, KeyCode::Left, KeyModifiers::ALT);
    assert_eq!(app.cursor, 0);
  }

  #[test]
  /// Verifies Alt+Right navigates one word to the right.
  fn alt_right_moves_cursor_one_word_right() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    for ch in "start 1 local".chars() {
      press(&mut app, KeyCode::Char(ch));
    }
    press(&mut app, KeyCode::Home);
    assert_eq!(app.cursor, 0);

    press_with_modifiers(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(app.cursor, "start".len());

    press_with_modifiers(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(app.cursor, "start 1".len());

    press_with_modifiers(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(app.cursor, app.input.len());

    // Should not go beyond the end.
    press_with_modifiers(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(app.cursor, app.input.len());
  }

  #[test]
  /// Verifies Alt+Left/Right handles non-ASCII words correctly.
  fn alt_arrow_word_navigation_handles_non_ascii() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    for ch in "café résumé".chars() {
      press(&mut app, KeyCode::Char(ch));
    }

    press_with_modifiers(&mut app, KeyCode::Left, KeyModifiers::ALT);
    assert_eq!(app.cursor, "café ".len());

    press_with_modifiers(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(app.cursor, app.input.len());
  }

  /// Returns a unique temp path for file-logging tests.
  fn temp_log_path() -> PathBuf {
    let base = std::env::current_dir().expect("cwd");
    let timestamp = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("time")
      .as_nanos();
    let sequence = TEMP_LOG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    base.join(format!(".tmp-ocppsim-log-{pid}-{timestamp}-{sequence}.log"))
  }

  fn press(app: &mut TerminalApp, code: KeyCode) {
    press_with_modifiers(app, code, KeyModifiers::NONE);
  }

  fn press_with_modifiers(
    app: &mut TerminalApp,
    code: KeyCode,
    modifiers: KeyModifiers,
  ) {
    let action = app.handle_key_event(KeyEvent::new(code, modifiers));
    assert!(matches!(action, InputAction::None));
  }
}
