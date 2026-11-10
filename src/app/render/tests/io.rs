//! Regression tests for terminal update errors.

use super::super::{InputKind, PromptRenderer};
use crate::app::TerminalApp;
use crate::ocpp::OcppVersion;
use std::io::{self, Write};

#[derive(Default)]
struct FailingWriter {
  bytes: Vec<u8>,
  fail_after: Option<usize>,
  fail_flush: bool,
  flushes: usize,
}

impl Write for FailingWriter {
  fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
    if let Some(limit) = self.fail_after {
      if self.bytes.len() >= limit {
        self.fail_after = None;
        return Err(io::Error::other("write error"));
      }
      let len = bytes.len().min(limit - self.bytes.len());
      self.bytes.extend_from_slice(&bytes[..len]);
      return Ok(len);
    }
    self.bytes.extend_from_slice(bytes);
    Ok(bytes.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    self.flushes += 1;
    if self.fail_flush {
      Err(io::Error::other("flush error"))
    } else {
      Ok(())
    }
  }
}

#[test]
fn paint_error_still_ends_and_flushes_the_update() {
  let mut renderer = PromptRenderer::new(FailingWriter {
    fail_after: Some(24),
    fail_flush: true,
    ..FailingWriter::default()
  });
  let mut app = TerminalApp::new(OcppVersion::V2_1);
  let error = renderer.draw(&mut app, (80, 24)).unwrap_err();
  assert_eq!(error.to_string(), "write error");
  assert!(renderer.stdout.bytes.ends_with(b"\x1b[?2026l"));
  assert_eq!(renderer.stdout.flushes, 1);
  assert!(app.view_dirty);
}

#[test]
fn begin_error_skips_paint_but_still_attempts_cleanup() {
  let mut renderer = PromptRenderer::new(FailingWriter {
    fail_after: Some(1),
    ..FailingWriter::default()
  });
  let mut painted = false;
  let error = renderer
    .with_synchronized_update(|_| {
      painted = true;
      Ok(())
    })
    .unwrap_err();
  assert_eq!(error.to_string(), "write error");
  assert!(!painted);
  assert!(renderer.stdout.bytes.ends_with(b"\x1b[?2026l"));
  assert_eq!(renderer.stdout.flushes, 1);
}

#[test]
fn cleanup_error_does_not_prevent_a_flush_attempt() {
  let mut renderer = PromptRenderer::new(FailingWriter::default());
  let error = renderer
    .with_synchronized_update(|renderer| {
      renderer.stdout.write_all(b"painted")?;
      renderer.stdout.fail_after = Some(renderer.stdout.bytes.len());
      Ok(())
    })
    .unwrap_err();
  assert_eq!(error.to_string(), "write error");
  assert_eq!(renderer.stdout.flushes, 1);
}

#[test]
fn successful_paint_reports_flush_error() {
  let mut renderer = PromptRenderer::new(FailingWriter {
    fail_flush: true,
    ..FailingWriter::default()
  });
  let error = renderer.with_synchronized_update(|_| Ok(())).unwrap_err();
  assert_eq!(error.to_string(), "flush error");
  assert!(renderer.stdout.bytes.ends_with(b"\x1b[?2026l"));
}

#[test]
fn view_invalidation_covers_input_logs_status_modals_and_resize() {
  use crate::simulator::{SimulatorRuntimeState, UiEvent};
  use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

  let mut renderer = PromptRenderer::new(Vec::new());
  let mut app = TerminalApp::new(OcppVersion::V2_1);
  renderer.draw(&mut app, (80, 24)).unwrap();
  assert!(!app.view_dirty);
  renderer.stdout.clear();
  for _ in 0..10 {
    renderer.draw(&mut app, (80, 24)).unwrap();
    assert_eq!(renderer.stdout, Vec::<u8>::new());
  }
  app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
  assert!(app.view_dirty);
  renderer.draw(&mut app, (80, 24)).unwrap();
  assert!(renderer.rendered.as_ref().unwrap().overlay.is_some());
  assert!(!app.view_dirty);

  app.push_info("new log");
  assert!(app.view_dirty);
  renderer.draw(&mut app, (80, 24)).unwrap();
  assert!(!app.view_dirty);
  app.set_connection_target(Some("demo".to_string()), None);
  assert!(app.view_dirty);
  renderer.draw(&mut app, (80, 24)).unwrap();
  app.apply(UiEvent::RuntimeState(SimulatorRuntimeState {
    connected: true,
    active_transactions: 1,
    ..SimulatorRuntimeState::default()
  }));
  assert!(app.view_dirty);
  renderer.draw(&mut app, (80, 24)).unwrap();
  assert!(
    renderer
      .rendered
      .as_ref()
      .unwrap()
      .prompt
      .taskbar
      .ends_with("| connected")
  );

  assert!(!app.request_exit());
  assert!(app.view_dirty);
  renderer.draw(&mut app, (80, 24)).unwrap();
  assert_eq!(
    renderer.rendered.as_ref().unwrap().prompt.input_kind,
    InputKind::Confirmation
  );
  app.cancel_exit_confirmation();
  assert!(app.view_dirty);
  renderer.draw(&mut app, (80, 24)).unwrap();
  assert_eq!(
    renderer.rendered.as_ref().unwrap().prompt.input_kind,
    InputKind::Normal
  );

  app.clear_logs();
  assert!(app.view_dirty);
  renderer.draw(&mut app, (80, 24)).unwrap();
  assert!(!app.view_dirty);
  renderer.stdout.clear();
  renderer.draw(&mut app, (40, 12)).unwrap();
  assert_ne!(renderer.stdout, Vec::<u8>::new());
  assert_eq!(renderer.rendered.as_ref().unwrap().size, (40, 12));
}
