use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Sink;
use tokio_tungstenite::tungstenite::{Error, Message};

use super::*;

struct StalledSink;

impl Sink<Message> for StalledSink {
  type Error = Error;

  fn poll_ready(
    self: Pin<&mut Self>,
    _cx: &mut Context<'_>,
  ) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn start_send(self: Pin<&mut Self>, _item: Message) -> Result<(), Error> {
    Ok(())
  }

  fn poll_flush(
    self: Pin<&mut Self>,
    _cx: &mut Context<'_>,
  ) -> Poll<Result<(), Self::Error>> {
    Poll::Pending
  }

  fn poll_close(
    self: Pin<&mut Self>,
    _cx: &mut Context<'_>,
  ) -> Poll<Result<(), Self::Error>> {
    Poll::Pending
  }
}

#[tokio::test]
async fn stalled_text_and_pong_writes_disconnect_on_timeout() {
  let mut simulator = simulator_for_tests();
  simulator.config.request_timeout = Duration::from_millis(10);
  let mut write = StalledSink;
  let error = tokio::time::timeout(
    Duration::from_secs(1),
    simulator.send_text(
      &mut write,
      "[]".to_string(),
      UiLogLevel::Tx,
      "test".to_string(),
    ),
  )
  .await
  .expect("text write must respect its deadline")
  .expect_err("stalled text write should fail");
  assert!(error.to_string().contains("WebSocket write timed out"));

  let mut outcome = crate::simulator::CommandOutcome::Continue;
  let disconnected = tokio::time::timeout(
    Duration::from_secs(1),
    crate::simulator::handle_connected_ws_message(
      &mut simulator,
      Some(Ok(Message::Ping(Vec::new().into()))),
      &mut write,
      &mut outcome,
    ),
  )
  .await
  .expect("pong write must respect its deadline");
  assert!(
    disconnected,
    "a partially written connection must be dropped"
  );
}

#[tokio::test]
async fn stalled_close_write_respects_deadline() {
  let mut simulator = simulator_for_tests();
  simulator.config.request_timeout = Duration::from_millis(10);
  tokio::time::timeout(
    Duration::from_secs(1),
    simulator.close_connection(&mut StalledSink),
  )
  .await
  .expect("close write must respect its deadline");
}

#[tokio::test]
async fn dropping_simulator_cancels_heartbeat_task() {
  let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
  let (ui_tx, _ui_rx) = mpsc::unbounded_channel();
  let mut simulator =
    Simulator::new(simulator_test_config(OcppVersion::V1_6), ui_tx, cmd_tx);
  simulator.start_heartbeat(10);
  assert!(matches!(
    cmd_rx.recv().await,
    Some(SimulatorCommand::HeartbeatTick)
  ));
  drop(simulator);

  assert!(
    tokio::time::timeout(Duration::from_secs(1), cmd_rx.recv())
      .await
      .expect("heartbeat sender must be dropped on cancellation")
      .is_none()
  );
}
