use std::collections::VecDeque;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{self, BufWriter, Stdout, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;
use crossterm::cursor::{
  MoveDown, MoveTo, MoveToColumn, MoveUp, SetCursorStyle, Show,
};
use crossterm::event::{
  self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::queue;
use crossterm::style::{
  Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor,
};
use crossterm::terminal::{
  self, Clear, ClearType, disable_raw_mode, enable_raw_mode,
};
use ratatui::text::Line;

use crate::ocpp::OcppVersion;
use crate::simulator::{
  ConnectorSnapshot, SimulatorSnapshot, UiEvent, UiLogLevel,
};

mod completion;
mod history;

use completion::{CompletionState, completion_seed};
use history::CommandHistory;

const MAX_LOG_LINES: usize = 10_000;
const PROMPT_PREFIX_WIDTH: usize = 2;

/// Number of terminal lines occupied by the live prompt block.
const PROMPT_BLOCK_LINES: u16 = 4;

pub struct TerminalSession {
  stdout: Stdout,
  prompt_visible: bool,
  last_prompt: Option<PromptSnapshot>,
}

impl TerminalSession {
  /// Initializes raw input mode while keeping normal terminal scrollback.
  pub fn new() -> Result<Self> {
    enable_raw_mode()?;
    Ok(Self {
      stdout: io::stdout(),
      prompt_visible: false,
      last_prompt: None,
    })
  }

  /// Flushes pending log lines and redraws the live command composer.
  pub fn draw(&mut self, app: &mut TerminalApp) -> Result<()> {
    let width = terminal_width();
    let prompt = PromptSnapshot::new(app, width);
    let should_clear_screen = app.take_screen_clear_requested();
    let logs = app.drain_pending_logs();
    let prompt_changed = self.last_prompt.as_ref() != Some(&prompt);

    if logs.is_empty() && !prompt_changed && !should_clear_screen {
      return Ok(());
    }

    self.clear_prompt()?;
    if should_clear_screen {
      execute!(self.stdout, Clear(ClearType::All), MoveTo(0, 0))?;
    }
    for entry in logs {
      self.write_log_entry(&entry)?;
    }
    self.render_prompt(app, width)?;
    self.last_prompt = Some(prompt);
    self.stdout.flush()?;
    Ok(())
  }

  /// Clears the rendered prompt block before logs or a fresh prompt redraw.
  fn clear_prompt(&mut self) -> io::Result<()> {
    if !self.prompt_visible {
      return Ok(());
    }
    queue!(
      self.stdout,
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      MoveUp(1),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      MoveDown(2),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      MoveDown(1),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      MoveUp(PROMPT_BLOCK_LINES - 1),
      MoveToColumn(0)
    )?;
    self.prompt_visible = false;
    self.last_prompt = None;
    Ok(())
  }

  /// Prints one log entry into normal terminal scrollback.
  fn write_log_entry(&mut self, entry: &LogEntry) -> io::Result<()> {
    queue!(
      self.stdout,
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      SetForegroundColor(log_level_color(entry.level)),
      Print(entry.formatted.as_str()),
      ResetColor,
      Print("\r\n")
    )
  }

  /// Renders the inline command composer and taskbar block.
  fn render_prompt(
    &mut self,
    app: &TerminalApp,
    width: usize,
  ) -> io::Result<()> {
    let content_width = width.saturating_sub(1);
    let input_width = content_width.saturating_sub(PROMPT_PREFIX_WIDTH);
    let view = visible_input_view(app.input(), app.cursor(), input_width);

    queue!(
      self.stdout,
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      Print("\r\n"),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      SetAttribute(Attribute::Bold),
      Print(">"),
      SetAttribute(Attribute::Reset),
      Print(" ")
    )?;

    if app.input().is_empty() {
      queue!(
        self.stdout,
        SetAttribute(Attribute::Dim),
        Print("Command"),
        SetAttribute(Attribute::Reset)
      )?;
    } else {
      queue!(self.stdout, Print(view.text))?;
    }

    let cursor_column = PROMPT_PREFIX_WIDTH
      .saturating_add(view.cursor_width)
      .min(content_width);
    queue!(
      self.stdout,
      Print("\r\n"),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine),
      Print("\r\n"),
      MoveToColumn(0),
      Clear(ClearType::CurrentLine)
    )?;
    self.render_taskbar(app, width)?;
    queue!(
      self.stdout,
      MoveUp(2),
      SetCursorStyle::DefaultUserShape,
      Show,
      MoveToColumn(u16::try_from(cursor_column).unwrap_or(u16::MAX))
    )?;
    self.prompt_visible = true;
    Ok(())
  }

  /// Renders the one-line profile and connection status taskbar.
  fn render_taskbar(
    &mut self,
    app: &TerminalApp,
    width: usize,
  ) -> io::Result<()> {
    let line = fit_to_width(&app.taskbar_line(), width.saturating_sub(1));
    queue!(
      self.stdout,
      SetAttribute(Attribute::Dim),
      Print(line),
      SetAttribute(Attribute::Reset)
    )
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
    let _ = self.clear_prompt();
    restore_console();
    let _ = execute!(
      self.stdout,
      ResetColor,
      SetCursorStyle::DefaultUserShape,
      Show
    );
  }
}

/// Restores console modes used by the inline terminal UI.
pub fn restore_console() {
  let _ = disable_raw_mode();
  let mut stdout = io::stdout();
  let _ = execute!(stdout, ResetColor, SetCursorStyle::DefaultUserShape, Show);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptSnapshot {
  /// Input text currently shown in the command composer.
  input: String,
  /// Byte cursor position within the input text.
  cursor: usize,
  /// Terminal width used for the last prompt render.
  width: usize,
  /// Profile name shown in the taskbar, when profile mode is active.
  profile_name: Option<String>,
  /// WebSocket URL shown in the taskbar when no profile name is available.
  ws_url: String,
  /// Connection state shown in the taskbar.
  connected: bool,
}

impl PromptSnapshot {
  fn new(app: &TerminalApp, width: usize) -> Self {
    Self {
      input: app.input().to_string(),
      cursor: app.cursor(),
      width,
      profile_name: app.profile_name.clone(),
      ws_url: app.ws_url.clone(),
      connected: app.connected,
    }
  }
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

fn display_width(input: &str) -> usize {
  Line::raw(input.to_string()).width()
}

/// Truncates display text so taskbar content never wraps.
fn fit_to_width(input: &str, width: usize) -> String {
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

fn terminal_width() -> usize {
  terminal::size()
    .map_or(80, |(width, _)| width as usize)
    .max(PROMPT_PREFIX_WIDTH)
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
  /// Profile name displayed in the inline taskbar, when profile mode is used.
  profile_name: Option<String>,
  /// WebSocket URL displayed in the inline taskbar without a profile name.
  ws_url: String,
  /// Latest simulator connection state displayed in the inline taskbar.
  connected: bool,
  pending_logs: VecDeque<LogEntry>,
  input: String,
  cursor: usize,
  history: CommandHistory,
  known_connectors: Vec<u16>,
  completion: Option<CompletionState>,
  log_sink: Option<FileLogSink>,
  screen_clear_requested: bool,
}

impl TerminalApp {
  /// Creates a fresh terminal app state for the selected protocol.
  pub fn new(protocol: OcppVersion) -> Self {
    Self {
      protocol,
      profile_name: None,
      ws_url: String::new(),
      connected: false,
      pending_logs: VecDeque::new(),
      input: String::new(),
      cursor: 0,
      history: CommandHistory::new(),
      known_connectors: Vec::new(),
      completion: None,
      log_sink: None,
      screen_clear_requested: false,
    }
  }

  /// Sets the connection target shown in the inline taskbar.
  pub fn set_connection_target(
    &mut self,
    profile_name: Option<String>,
    ws_url: String,
  ) {
    self.profile_name = profile_name;
    self.ws_url = ws_url;
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

  /// Clears pending log redraws and asks the terminal to clear the visible
  /// screen while keeping normal terminal scrollback available.
  pub fn clear_logs(&mut self) {
    self.pending_logs.clear();
    self.screen_clear_requested = true;
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
      KeyCode::PageUp | KeyCode::PageDown => {
        self.clear_completion();
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

  /// Handles Alt-modified shortcuts for word movement.
  fn handle_alt_key(&mut self, key: KeyEvent) -> InputAction {
    self.clear_completion();
    match key.code {
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

  /// Clears any active tab-completion state.
  fn clear_completion(&mut self) {
    self.completion = None;
  }

  fn input(&self) -> &str {
    &self.input
  }

  fn cursor(&self) -> usize {
    self.cursor
  }

  fn drain_pending_logs(&mut self) -> Vec<LogEntry> {
    self.pending_logs.drain(..).collect()
  }

  fn take_screen_clear_requested(&mut self) -> bool {
    let requested = self.screen_clear_requested;
    self.screen_clear_requested = false;
    requested
  }

  /// Builds the compact taskbar text shown below the command composer.
  fn taskbar_line(&self) -> String {
    let target = self.profile_name.as_ref().map_or_else(
      || format!("url {}", display_taskbar_value(&self.ws_url)),
      |name| format!("profile {}", display_taskbar_value(name)),
    );
    let status = if self.connected {
      "connected"
    } else {
      "disconnected"
    };
    format!(" {target} | {status}")
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
  /// is appended to pending terminal output.
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
  }

  /// Appends one pending terminal log entry.
  fn push_log_entry(&mut self, mut entry: LogEntry) {
    entry.formatted = Self::format_log_entry(&entry);
    self.pending_logs.push_back(entry);
    while self.pending_logs.len() > MAX_LOG_LINES {
      let _ = self.pending_logs.pop_front();
    }
  }

  /// Applies a simulator snapshot by logging summary and connector details.
  fn push_snapshot(&mut self, snapshot: SimulatorSnapshot) {
    self.known_connectors =
      snapshot.connectors.iter().map(|item| item.id).collect();
    self.ws_url.clone_from(&snapshot.connection_url);
    self.connected = snapshot.connected;

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

/// Displays empty taskbar values as a stable placeholder.
fn display_taskbar_value(value: &str) -> &str {
  if value.is_empty() { "-" } else { value }
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

  use super::{InputAction, TerminalApp, visible_input_view};

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
  /// Verifies queued log lines are drained after terminal rendering.
  fn drains_pending_logs_for_terminal_output() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);

    app.push_info("hello");
    app.push_error("oops");

    let logs = app.drain_pending_logs();
    assert_eq!(logs.len(), 2);
    assert!(logs[0].formatted.contains("[INFO] hello"));
    assert!(logs[1].formatted.contains("[ERROR] oops"));
    assert!(app.drain_pending_logs().is_empty());
  }

  #[test]
  /// Verifies the taskbar prefers the active profile name.
  fn taskbar_prefers_profile_name() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.set_connection_target(
      Some("demo".to_string()),
      "ws://example.test/ocpp".to_string(),
    );

    assert_eq!(app.taskbar_line(), " profile demo | disconnected");

    app.connected = true;
    assert_eq!(app.taskbar_line(), " profile demo | connected");
  }

  #[test]
  /// Verifies the taskbar falls back to the WebSocket URL.
  fn taskbar_uses_ws_url_without_profile() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.set_connection_target(None, "ws://example.test/ocpp".to_string());

    assert_eq!(
      app.taskbar_line(),
      " url ws://example.test/ocpp | disconnected"
    );
  }

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
  /// Verifies non-ASCII insertion and deletion keep byte indices valid.
  fn edits_non_ascii_input_on_character_boundaries() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);

    press(&mut app, KeyCode::Char('é'));
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.input, "éx");
    assert_eq!(app.cursor, app.input.len());
    assert_eq!(super::display_width(&app.input[..app.cursor]), 2);

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
