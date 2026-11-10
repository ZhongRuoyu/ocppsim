use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{self, BufWriter, Stdout, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;
use crossterm::cursor::{self, SetCursorStyle, Show};
use crossterm::event::{
  self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::style::ResetColor;
use crossterm::terminal::{
  self, EndSynchronizedUpdate, disable_raw_mode, enable_raw_mode,
};

use crate::ocpp::OcppVersion;
use crate::sensitive::{redact_text_secrets, redact_url_secrets};
use crate::simulator::{
  ConfigurationSnapshot, ConnectorSnapshot, SimulatorRuntimeState,
  SimulatorSnapshot, UiEvent, UiLogLevel,
};

mod completion;
mod history;
mod log_tail;
mod render;

use completion::{
  CompletionState, completion_seed, completion_suffix,
  configuration_completion_seed,
};
use history::CommandHistory;

const MAX_LOG_LINES: usize = 10_000;
const EXIT_CONFIRMATION_PROMPT: &str =
  "Active CSMS state may remain unfinished. Confirm exit? [y/N]";

pub struct TerminalSession {
  renderer: render::PromptRenderer<BufWriter<Stdout>>,
}

impl TerminalSession {
  /// Initializes raw input mode while keeping normal terminal scrollback.
  pub fn new() -> Result<Self> {
    enable_raw_mode()?;
    // Buffer frame output before it reaches line-buffered stdout.
    let mut renderer =
      render::PromptRenderer::new(BufWriter::new(io::stdout()));
    renderer.set_cursor_row(cursor::position().ok().map(|(_, row)| row));
    Ok(Self { renderer })
  }

  /// Flushes logs and redraws the prompt and any completion overlay.
  pub fn draw(&mut self, app: &mut TerminalApp) -> Result<()> {
    let size = terminal::size().unwrap_or((80, 24));
    if self.renderer.needs_cursor_position(size) {
      self
        .renderer
        .set_cursor_row(cursor::position().ok().map(|(_, row)| row));
    }
    self.renderer.draw(app, size)?;
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
    let _ = self
      .renderer
      .clear_prompt(terminal::size().unwrap_or((80, 24)));
    let _ = self.renderer.stdout.flush();
    restore_console();
    let _ = execute!(
      self.renderer.stdout,
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
  let _ = execute!(
    stdout,
    EndSynchronizedUpdate,
    ResetColor,
    SetCursorStyle::DefaultUserShape,
    Show
  );
}

#[derive(Debug, Clone)]
struct LogEntry {
  timestamp: String,
  level: UiLogLevel,
  message: String,
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

#[derive(Debug, Clone)]
struct ExitConfirmation {
  signature: SimulatorRuntimeState,
  confirmed: bool,
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
  configuration: Vec<ConfigurationSnapshot>,
  profile_completions: Vec<String>,
  completion: Option<CompletionState>,
  log_sink: Option<FileLogSink>,
  screen_clear_requested: bool,
  view_dirty: bool,
  runtime_state: SimulatorRuntimeState,
  exit_confirmation: Option<ExitConfirmation>,
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
      configuration: Vec::new(),
      profile_completions: Vec::new(),
      completion: None,
      log_sink: None,
      screen_clear_requested: false,
      view_dirty: true,
      runtime_state: SimulatorRuntimeState::default(),
      exit_confirmation: None,
    }
  }

  /// Sets the connection target shown in the inline taskbar.
  pub fn set_connection_target(
    &mut self,
    profile_name: Option<String>,
    ws_url: Option<String>,
  ) {
    self.invalidate_view();
    self.profile_name = profile_name;
    self.ws_url = ws_url
      .map(|url| redact_url_secrets(&url))
      .unwrap_or_default();
  }

  /// Returns the active protocol for command help and completions.
  pub fn protocol(&self) -> OcppVersion {
    self.protocol
  }

  /// Sets profile names offered for `connect <profile>` completion.
  pub fn set_profile_completions(&mut self, profiles: Vec<String>) {
    self.invalidate_view();
    self.profile_completions = profiles;
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
      UiEvent::RuntimeState(state) => self.apply_runtime_state(state),
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
    self.invalidate_view();
    self.pending_logs.clear();
    self.screen_clear_requested = true;
  }

  /// Requests process exit, requiring confirmation when runtime state is
  /// active enough that shutdown may leave the CSMS in an unfinished state.
  pub fn request_exit(&mut self) -> bool {
    self.invalidate_view();
    if !exit_requires_confirmation(&self.runtime_state) {
      self.exit_confirmation = None;
      return true;
    }

    if self.exit_confirmation_confirmed_for_current_state() {
      self.exit_confirmation = None;
      return true;
    }

    self.exit_confirmation = Some(ExitConfirmation {
      signature: self.runtime_state.clone(),
      confirmed: false,
    });
    false
  }

  /// Cancels any pending exit confirmation.
  pub fn cancel_exit_confirmation(&mut self) {
    if self.exit_confirmation.take().is_some() {
      self.invalidate_view();
    }
  }

  /// Handles keyboard input for editing, history, completion, and submit.
  pub fn handle_key_event(&mut self, key: KeyEvent) -> InputAction {
    self.invalidate_view();
    if let Some(action) = self.handle_exit_confirmation_key(key) {
      return action;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
      return self.handle_ctrl_key(key);
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
      return self.handle_alt_key(key);
    }

    self.handle_plain_key_event(key)
  }

  fn handle_exit_confirmation_key(
    &mut self,
    key: KeyEvent,
  ) -> Option<InputAction> {
    self.exit_confirmation.as_ref()?;

    if is_exit_confirmation_yes(key) {
      if let Some(confirmation) = self.exit_confirmation.as_mut() {
        confirmation.confirmed = true;
      }
      Some(InputAction::ExitRequested)
    } else if key.modifiers.contains(KeyModifiers::CONTROL)
      && matches!(key.code, KeyCode::Char('c' | 'd'))
    {
      Some(InputAction::None)
    } else {
      self.exit_confirmation = None;
      Some(InputAction::None)
    }
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
        if self.completion.take().is_some() {
          return InputAction::None;
        }
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
      KeyCode::Char('c' | 'd') => {
        self.clear_completion();
        InputAction::ExitRequested
      }
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

  /// Completes at the cursor using only preceding text to select candidates.
  ///
  /// When `reverse` is `true`, candidates are traversed backward.
  fn complete_input(&mut self, reverse: bool) {
    if let Some(state) = self.completion.as_mut() {
      if state.is_empty() {
        self.completion = None;
        return;
      }
      (self.input, self.cursor) = state.next_value(reverse);
      return;
    }

    let prefix = &self.input[..self.cursor];
    let Some((base, candidates)) =
      configuration_completion_seed(prefix, &self.configuration).or_else(
        || {
          completion_seed(
            prefix,
            self.protocol,
            &self.known_connectors,
            &self.profile_completions,
          )
        },
      )
    else {
      return;
    };
    if candidates.is_empty() {
      return;
    }

    let index = if reverse { candidates.len() - 1 } else { 0 };

    let multiple = candidates.len() > 1;
    let suffix = completion_suffix(&self.input, self.cursor).to_string();
    let state = CompletionState::new(base, candidates, index, suffix);
    (self.input, self.cursor) = state.value();
    if multiple {
      self.completion = Some(state);
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
    if self.completion.take().is_some() {
      self.invalidate_view();
    }
  }

  /// Invalidates the prepared view, coalescing a batch of UI changes.
  fn invalidate_view(&mut self) {
    self.view_dirty = true;
  }

  fn take_view_dirty(&mut self) -> bool {
    std::mem::take(&mut self.view_dirty)
  }

  fn prompt_shadow(&self) -> Option<&str> {
    self
      .exit_confirmation
      .as_ref()
      .map(|_| EXIT_CONFIRMATION_PROMPT)
  }

  fn apply_runtime_state(&mut self, state: SimulatorRuntimeState) {
    self.invalidate_view();
    if self
      .exit_confirmation
      .as_ref()
      .is_some_and(|confirmation| confirmation.signature != state)
    {
      self.exit_confirmation = None;
    }
    self.connected = state.connected;
    self.runtime_state = state;
  }

  fn exit_confirmation_confirmed_for_current_state(&self) -> bool {
    self.exit_confirmation.as_ref().is_some_and(|confirmation| {
      confirmation.confirmed && confirmation.signature == self.runtime_state
    })
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
    format_taskbar_line(
      self.profile_name.as_deref(),
      &self.ws_url,
      self.connected,
    )
  }

  /// Pushes one or more log lines to memory and optional file sink.
  ///
  /// Multi-line messages are split so each line gets its own timestamped
  /// entry. On file write failure, file logging is disabled and an error line
  /// is appended to pending terminal output.
  fn push_log<S: Into<String>>(&mut self, level: UiLogLevel, message: S) {
    let message = message.into();
    let message = redact_text_secrets(&message);
    let mut sink_error: Option<String> = None;

    for line in message.lines() {
      let timestamp = log_timestamp_now();
      self.push_log_entry(LogEntry {
        timestamp: timestamp.clone(),
        level,
        message: line.to_string(),
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
  fn push_log_entry(&mut self, entry: LogEntry) {
    self.invalidate_view();
    self.pending_logs.push_back(entry);
    while self.pending_logs.len() > MAX_LOG_LINES {
      let _ = self.pending_logs.pop_front();
    }
  }

  /// Applies a simulator snapshot by logging summary and connector details.
  fn push_snapshot(&mut self, snapshot: SimulatorSnapshot) {
    let active_transactions = snapshot
      .connectors
      .iter()
      .filter(|item| item.transaction.is_some())
      .count();
    self.apply_runtime_state(SimulatorRuntimeState {
      connected: snapshot.connected,
      queue_depth: snapshot.queue_depth,
      pending_action: snapshot.pending_action.clone(),
      active_transactions,
      pending_reconnect: self.runtime_state.pending_reconnect,
    });

    self.known_connectors =
      snapshot.connectors.iter().map(|item| item.id).collect();
    self.configuration = snapshot.configuration;
    self.profile_name.clone_from(&snapshot.profile);
    self.ws_url.clone_from(&snapshot.connection_url);
    self.connected = snapshot.connected;
    self.protocol = snapshot.protocol;

    self.push_log(
      UiLogLevel::Info,
      format!(
        "CP {} protocol={} connected={} heartbeat={} queue={} pending={}",
        display_taskbar_value(snapshot.cp_id.as_deref().unwrap_or("")),
        snapshot.protocol.label(),
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
  let mut measurements = String::new();
  for reading in &connector.meter_values {
    if reading.measurand == crate::ocpp::Measurand::EnergyActiveImportRegister {
      continue;
    }
    write!(
      measurements,
      " {}={}{}",
      reading.measurand.as_str(),
      reading.value,
      reading.unit,
    )
    .expect("formatting a string cannot fail");
  }
  format!(
    "Connector {} status={} meter={}Wh{} tx={}",
    connector.id, connector.status, connector.meter_wh, measurements, tx,
  )
}

/// Displays empty taskbar values as a stable placeholder.
fn display_taskbar_value(value: &str) -> &str {
  if value.is_empty() { "-" } else { value }
}

fn format_taskbar_line(
  profile_name: Option<&str>,
  ws_url: &str,
  connected: bool,
) -> String {
  let target = profile_name.map_or_else(
    || format!("url {}", display_taskbar_value(ws_url)),
    |name| format!("profile {}", display_taskbar_value(name)),
  );
  let status = if connected {
    "connected"
  } else {
    "disconnected"
  };
  format!(" {target} | {status}")
}

/// Formats heartbeat interval for display.
fn display_heartbeat(value: Option<u64>) -> String {
  value.map_or_else(|| "-".to_string(), |seconds| format!("{seconds}s"))
}

fn exit_requires_confirmation(state: &SimulatorRuntimeState) -> bool {
  state.active_transactions > 0
    || state.pending_reconnect
    || (state.connected
      && (state.queue_depth > 0 || state.pending_action.is_some()))
}

fn is_exit_confirmation_yes(key: KeyEvent) -> bool {
  matches!(key.code, KeyCode::Char('y' | 'Y'))
    && (key.modifiers == KeyModifiers::NONE
      || key.modifiers == KeyModifiers::SHIFT)
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

  use crate::ocpp::{ConfigurationKey, OcppVersion};
  use crate::simulator::{
    ConfigurationSnapshot, SimulatorRuntimeState, UiEvent,
  };

  use super::render::layout::display_width;
  use super::{InputAction, TerminalApp};

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
    assert_eq!(logs[0].level.label(), "INFO");
    assert_eq!(logs[0].message, "hello");
    assert_eq!(logs[1].level.label(), "ERROR");
    assert_eq!(logs[1].message, "oops");
    assert!(app.drain_pending_logs().is_empty());
  }

  #[test]
  /// Verifies safe exits do not ask for confirmation.
  fn safe_exit_requests_complete_immediately() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);

    assert!(app.request_exit());
    assert!(app.prompt_shadow().is_none());
  }

  #[test]
  /// Verifies risky exits shadow the input line and preserve draft text.
  fn risky_exit_request_confirms_on_y_without_enter() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
      active_transactions: 1,
      ..SimulatorRuntimeState::default()
    }));
    for ch in "meter 1 100".chars() {
      press(&mut app, KeyCode::Char(ch));
    }

    assert!(!app.request_exit());
    assert_eq!(app.prompt_shadow(), Some(super::EXIT_CONFIRMATION_PROMPT));
    assert_eq!(app.input, "meter 1 100");

    let action = app
      .handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    assert!(matches!(action, InputAction::ExitRequested));
    assert!(app.request_exit());
  }

  #[test]
  /// Verifies uppercase Y confirms exit without Enter.
  fn risky_exit_request_confirms_on_uppercase_y_without_enter() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
      active_transactions: 1,
      ..SimulatorRuntimeState::default()
    }));

    assert!(!app.request_exit());
    let action = app
      .handle_key_event(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT));

    assert!(matches!(action, InputAction::ExitRequested));
    assert!(app.request_exit());
  }

  #[test]
  /// Verifies Esc cancels an exit confirmation without clearing draft input.
  fn escape_cancels_exit_confirmation_prompt() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
      connected: true,
      queue_depth: 1,
      ..SimulatorRuntimeState::default()
    }));
    for ch in "status".chars() {
      press(&mut app, KeyCode::Char(ch));
    }

    assert!(!app.request_exit());
    press(&mut app, KeyCode::Esc);

    assert!(app.prompt_shadow().is_none());
    assert_eq!(app.input, "status");
    assert!(!app.request_exit());
  }

  #[test]
  /// Verifies non-yes keys cancel the prompt without editing draft input.
  fn non_yes_key_cancels_exit_confirmation_without_editing_input() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
      active_transactions: 1,
      ..SimulatorRuntimeState::default()
    }));
    for ch in "status".chars() {
      press(&mut app, KeyCode::Char(ch));
    }

    assert!(!app.request_exit());
    press(&mut app, KeyCode::Char('s'));
    assert!(app.prompt_shadow().is_none());
    assert_eq!(app.input, "status");

    let action =
      app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(action, InputAction::Submitted(line) if line == "status"));
  }

  #[test]
  /// Verifies Ctrl-C and Ctrl-D keep the confirmation prompt active.
  fn ctrl_exit_keys_keep_exit_confirmation_prompt_active() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
      active_transactions: 1,
      ..SimulatorRuntimeState::default()
    }));

    assert!(!app.request_exit());
    press_with_modifiers(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(app.prompt_shadow(), Some(super::EXIT_CONFIRMATION_PROMPT));

    press_with_modifiers(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);
    assert_eq!(app.prompt_shadow(), Some(super::EXIT_CONFIRMATION_PROMPT));
  }

  #[test]
  /// Verifies runtime-state updates are silent and reset stale confirmations.
  fn runtime_state_updates_are_silent_and_reset_exit_confirmation() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
      connected: true,
      pending_action: Some("BootNotification (m1)".to_string()),
      ..SimulatorRuntimeState::default()
    }));

    assert!(app.drain_pending_logs().is_empty());
    assert_eq!(app.taskbar_line(), " url - | connected");
    assert!(!app.request_exit());

    app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
      connected: true,
      ..SimulatorRuntimeState::default()
    }));

    assert!(app.prompt_shadow().is_none());
    assert!(app.request_exit());
  }

  #[test]
  /// Verifies the taskbar prefers the active profile name.
  fn taskbar_prefers_profile_name() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.set_connection_target(
      Some("demo".to_string()),
      Some("ws://example.test/ocpp".to_string()),
    );

    assert_eq!(app.taskbar_line(), " profile demo | disconnected");

    app.connected = true;
    assert_eq!(app.taskbar_line(), " profile demo | connected");
  }

  #[test]
  /// Verifies the taskbar falls back to the WebSocket URL.
  fn taskbar_uses_ws_url_without_profile() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.set_connection_target(None, Some("ws://example.test/ocpp".to_string()));

    assert_eq!(
      app.taskbar_line(),
      " url ws://example.test/ocpp | disconnected"
    );
  }

  #[test]
  /// Verifies taskbar URL display masks credentials.
  fn taskbar_redacts_ws_url_secrets() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.set_connection_target(
      None,
      Some("wss://user:secret@example.test/ocpp?token=SECRET".to_string()),
    );

    assert_eq!(
      app.taskbar_line(),
      " url wss://<redacted>@example.test/ocpp?token=<redacted> | disconnected"
    );
  }

  #[test]
  /// Verifies non-ASCII insertion and deletion keep byte indices valid.
  fn edits_non_ascii_input_on_character_boundaries() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);

    press(&mut app, KeyCode::Char('é'));
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.input, "éx");
    assert_eq!(app.cursor, app.input.len());
    assert_eq!(display_width(&app.input[..app.cursor]), 2);

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

  #[test]
  fn escape_dismisses_completion_before_clearing_input() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    press(&mut app, KeyCode::Tab);
    let completed = app.input.clone();
    let cursor = app.cursor;
    assert!(app.completion.is_some());
    press(&mut app, KeyCode::Esc);
    assert!(app.completion.is_none());
    assert_eq!(app.input, completed);
    assert_eq!(app.cursor, cursor);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.input, "");
  }

  #[test]
  fn enter_submits_the_selected_completion() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    press(&mut app, KeyCode::Tab);
    let action =
      app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(action, InputAction::Submitted(line) if line == "status"));
    assert!(app.completion.is_none());
  }

  #[test]
  fn tab_completion_preserves_alias_spellings_and_cycles_short_aliases() {
    for key in [KeyCode::Tab, KeyCode::BackTab] {
      let mut app = TerminalApp::new(OcppVersion::V2_1);
      app.input = "qu".to_string();
      app.cursor = app.input.len();
      press(&mut app, key);
      assert_eq!(app.input, "quit ");
      assert_eq!(app.cursor, 5);
      assert!(app.completion.is_none());
    }
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.input = "h".to_string();
    app.cursor = 1;
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.input, "h ");
    for expected in ["heartbeat ", "help ", "h "] {
      press(&mut app, KeyCode::Tab);
      assert_eq!(app.input, expected);
    }
    app.clear_completion();
    app.input = "?".to_string();
    app.cursor = 1;
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.input, "? ");
    assert!(app.completion.is_none());
  }

  #[test]
  fn unique_completion_does_not_leave_a_menu_open() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.input = "heart".to_string();
    app.cursor = app.input.len();
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.input, "heartbeat ");
    assert!(app.completion.is_none());
  }

  #[test]
  fn completion_replaces_the_current_word_and_cycles_at_the_cursor() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.input = "stale 1 caf\u{e9}".to_string();
    app.cursor = "sta".len();
    for (key, command) in [
      (KeyCode::Tab, "status"),
      (KeyCode::Tab, "start"),
      (KeyCode::BackTab, "status"),
      (KeyCode::BackTab, "standards"),
    ] {
      press(&mut app, key);
      assert_eq!(app.input, format!("{command} 1 caf\u{e9}"));
      assert_eq!(app.cursor, command.len());
    }
    press_with_modifiers(&mut app, KeyCode::Tab, KeyModifiers::SHIFT);
    assert_eq!(app.input, "start 1 caf\u{e9}");
    assert_eq!(app.cursor, "start".len());
  }

  #[test]
  fn completion_ignores_later_arguments_for_all_candidate_sources() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.known_connectors = vec![1, 2];
    app.set_profile_completions(vec!["caf\u{e9}".to_string()]);
    app.configuration = [
      ConfigurationKey::HeartbeatInterval,
      ConfigurationKey::AuthorizeRemoteTxRequests,
    ]
    .map(|key| ConfigurationSnapshot {
      key,
      read_only: false,
    })
    .to_vec();
    for (input, prefix, expected, cursor) in [
      ("connect 1", "", "status 1", "status"),
      ("conxx 1", "con", "config 1", "config"),
      ("start 9 token", "start ", "start 1 token", "start 1"),
      ("stop 1  Local", "stop 1", "stop 1  Local", "stop 1"),
      (
        "connect caf\u{e9}xxx next",
        "connect caf\u{e9}",
        "connect caf\u{e9} next",
        "connect caf\u{e9}",
      ),
      (
        "measurand 1 Socxxx 25",
        "measurand 1 So",
        "measurand 1 SoC 25",
        "measurand 1 SoC",
      ),
      (
        "config set Heartxxx 25",
        "config set Heart",
        "config set HeartbeatInterval 25",
        "config set HeartbeatInterval",
      ),
      (
        "config set AuthorizeRemoteTxRequests true extra",
        "config set AuthorizeRemoteTxRequests t",
        "config set AuthorizeRemoteTxRequests true extra",
        "config set AuthorizeRemoteTxRequests true",
      ),
    ] {
      app.input = input.to_string();
      app.cursor = prefix.len();
      app.clear_completion();
      press(&mut app, KeyCode::Tab);
      assert_eq!(app.input, expected, "{input}");
      assert_eq!(app.cursor, cursor.len(), "{input}");
    }
  }

  #[test]
  fn completion_preserves_later_protocol_members_and_existing_quotes() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.configuration = vec![ConfigurationSnapshot {
      key: ConfigurationKey::SupportedFileTransferProtocols,
      read_only: false,
    }];
    let command = "config set SupportedFileTransferProtocols ";
    for (value, prefix, expected, completed) in [
      ("Fxxx,FTP", "F", "FTP,FTP", "FTP"),
      (r#""Fxxx,FTP""#, r#""F"#, r#""FTP,FTP""#, r#""FTP"#),
      (r#""Hxxx""#, r#""H"#, r#""HTTP""#, r#""HTTP"#),
      (
        r#""HTTP,HTTPS,FTPS,Fxxx""#,
        r#""HTTP,HTTPS,FTPS,F"#,
        r#""HTTP,HTTPS,FTPS,FTP""#,
        r#""HTTP,HTTPS,FTPS,FTP"#,
      ),
      (
        r#""HTTP,HTTPS,FTPS,Fxxx ,FTP""#,
        r#""HTTP,HTTPS,FTPS,F"#,
        r#""HTTP,HTTPS,FTPS,FTP ,FTP""#,
        r#""HTTP,HTTPS,FTPS,FTP"#,
      ),
      (r#""HTTP, FTP""#, "", "HTTP", "HTTP"),
    ] {
      app.input = format!("{command}{value}");
      app.cursor = command.len() + prefix.len();
      app.clear_completion();
      press(&mut app, KeyCode::Tab);
      assert_eq!(app.input, format!("{command}{expected}"));
      assert_eq!(app.cursor, command.len() + completed.len());
      assert!(crate::commands::parse_command(&app.input).is_ok());
    }
  }

  #[test]
  fn completion_keeps_the_cursor_when_no_candidates_match() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    for (input, prefix) in [
      ("unknown argument", "unk"),
      ("connect missing next", "connect miss"),
      ("data-transfer vendor message", "data-transfer ven"),
    ] {
      app.input = input.to_string();
      app.cursor = prefix.len();
      press(&mut app, KeyCode::Tab);
      assert_eq!(app.input, input);
      assert_eq!(app.cursor, prefix.len());
    }
  }

  #[test]
  fn cursor_movement_and_edits_reset_completion_candidates() {
    let mut app = TerminalApp::new(OcppVersion::V2_1);
    app.input = "connect profile".to_string();
    app.cursor = "con".len();
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.input, "config profile");
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.input, "config profile");

    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.input, "status profile");
    assert_eq!(app.cursor, "status".len());
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
