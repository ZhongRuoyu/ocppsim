use std::io::ErrorKind;

use futures_util::StreamExt;
use http::HeaderValue;
use http::header::SEC_WEBSOCKET_PROTOCOL;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, accept_hdr_async};

use super::*;
use crate::ocpp::{OcppFrame, parse_frame};
use crate::simulator::types::SecurityReconnectPlan;
use crate::simulator::{
  Connection, OfflineOutcome, reconnect_after_security_change, run_simulator,
};

const TEST_DEADLINE: Duration = Duration::from_secs(2);

struct TestRuntime {
  commands: mpsc::UnboundedSender<SimulatorCommand>,
  events: mpsc::UnboundedReceiver<UiEvent>,
  task: JoinHandle<()>,
}

impl TestRuntime {
  fn new(config: SimulatorConfig) -> Self {
    let (commands, cmd_rx) = mpsc::unbounded_channel();
    let (ui_tx, events) = mpsc::unbounded_channel();
    let task =
      tokio::spawn(run_simulator(config, cmd_rx, ui_tx, commands.clone()));
    Self {
      commands,
      events,
      task,
    }
  }

  fn send(&self, command: SimulatorCommand) {
    self.commands.send(command).expect("send simulator command");
  }

  async fn wait_for_event(&mut self, matches: impl Fn(&UiEvent) -> bool) {
    timeout(TEST_DEADLINE, async {
      while let Some(event) = self.events.recv().await {
        if matches(&event) {
          return;
        }
      }
      panic!("simulator ended before expected event");
    })
    .await
    .expect("simulator must remain responsive");
  }

  async fn wait_for_log(&mut self, expected: &str) {
    self
      .wait_for_event(|event| {
        matches!(event, UiEvent::Log { message, .. }
          if message.contains(expected))
      })
      .await;
  }

  async fn shutdown(mut self) {
    self.send(SimulatorCommand::Shutdown);
    timeout(TEST_DEADLINE, &mut self.task)
      .await
      .expect("shutdown must cancel a pending connection")
      .expect("simulator task");
  }
}

impl Drop for TestRuntime {
  fn drop(&mut self) {
    // Also stop the simulator when a responsiveness assertion fails.
    self.task.abort();
  }
}

async fn pending_connection(
  scheme: &str,
  request_timeout: Duration,
) -> (TestRuntime, TcpStream, TcpListener) {
  crate::install_rustls_provider();
  let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
  let address = listener.local_addr().expect("address");
  let mut config = simulator_test_config(OcppVersion::V1_6);
  config.ws_url = Some(format!("{scheme}://{address}/ocpp/CP-TEST"));
  config.request_timeout = request_timeout;
  config.heartbeat_seconds = None;
  let runtime = TestRuntime::new(config);
  runtime.send(SimulatorCommand::Connect { config: None });
  let (socket, _) = timeout(TEST_DEADLINE, listener.accept())
    .await
    .expect("connection started")
    .expect("accept connection");
  (runtime, socket, listener)
}

async fn assert_connection_closed(socket: &mut TcpStream) {
  let mut received = Vec::new();
  let result = timeout(TEST_DEADLINE, socket.read_to_end(&mut received))
    .await
    .expect("cancellation must close the socket");
  if let Err(error) = result {
    assert_eq!(error.kind(), ErrorKind::ConnectionReset);
  }
  assert!(
    !String::from_utf8_lossy(&received).contains("BootNotification"),
    "canceled handshakes must not send OCPP messages"
  );
}

#[tokio::test]
async fn pending_handshake_keeps_commands_and_shutdown_responsive() {
  for scheme in ["ws", "wss"] {
    let (mut runtime, mut socket, _listener) =
      pending_connection(scheme, Duration::from_secs(30)).await;
    runtime.send(SimulatorCommand::Connect { config: None });
    runtime.send(SimulatorCommand::UpdateMeterValue {
      connector: 1,
      update: MeterUpdate::Set(MeterReading::energy(42)),
    });
    runtime.send(SimulatorCommand::Status);
    runtime.wait_for_log("Connection already in progress").await;
    runtime
      .wait_for_event(|event| {
        matches!(event, UiEvent::Snapshot(snapshot)
        if !snapshot.connected && snapshot.connectors.iter().any(
          |connector| connector.id == 1 && connector.meter_wh == 42
        ))
      })
      .await;
    runtime.shutdown().await;
    assert_connection_closed(&mut socket).await;
  }
}

#[tokio::test]
async fn disconnect_cancels_handshake_and_allows_a_new_connection() {
  let (mut runtime, mut first_socket, listener) =
    pending_connection("ws", Duration::from_secs(30)).await;
  runtime.send(SimulatorCommand::Disconnect);
  runtime.wait_for_log("Connection attempt canceled").await;
  assert_connection_closed(&mut first_socket).await;

  runtime.send(SimulatorCommand::Connect { config: None });
  let (socket, _) = timeout(TEST_DEADLINE, listener.accept())
    .await
    .expect("new connection started")
    .expect("accept new connection");
  // Tungstenite requires its unboxed HTTP error response in this callback.
  #[allow(clippy::result_large_err)]
  let callback = |request: &Request, mut response: Response| {
    assert_eq!(request.headers()[SEC_WEBSOCKET_PROTOCOL], "ocpp1.6");
    response
      .headers_mut()
      .insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static("ocpp1.6"));
    Ok(response)
  };
  let mut server = timeout(TEST_DEADLINE, accept_hdr_async(socket, callback))
    .await
    .expect("handshake deadline")
    .expect("server handshake");
  runtime.wait_for_log("Connected. Negotiated").await;
  let boot = timeout(TEST_DEADLINE, server.next())
    .await
    .expect("boot deadline")
    .expect("boot frame")
    .expect("read boot");
  assert!(matches!(
    parse_frame(boot.to_text().expect("boot text")).expect("OCPP frame"),
    OcppFrame::Call { action, .. } if action == "BootNotification"
  ));
  runtime.shutdown().await;
}

#[tokio::test]
async fn handshake_deadline_is_not_reset_by_incoming_commands() {
  for scheme in ["ws", "wss"] {
    let (mut runtime, mut socket, _listener) =
      pending_connection(scheme, Duration::from_millis(150)).await;
    let commands = runtime.commands.clone();
    let traffic = tokio::spawn(async move {
      while commands.send(SimulatorCommand::HeartbeatTick).is_ok() {
        tokio::time::sleep(Duration::from_millis(10)).await;
      }
    });
    runtime.wait_for_log("connection attempt timed out").await;
    traffic.abort();
    assert_connection_closed(&mut socket).await;
    runtime.send(SimulatorCommand::Status);
    runtime
      .wait_for_event(|event| {
        matches!(event, UiEvent::Snapshot(snapshot) if !snapshot.connected)
      })
      .await;
    runtime.shutdown().await;
  }
}

#[tokio::test]
async fn canceling_security_reconnect_does_not_start_fallback() {
  for command in [SimulatorCommand::Disconnect, SimulatorCommand::Shutdown] {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let client = TcpStream::connect(address).await.expect("initial client");
    let (_old_server, _) = listener.accept().await.expect("initial server");
    let stream = WebSocketStream::from_raw_socket(
      MaybeTlsStream::Plain(client),
      Role::Client,
      None,
    )
    .await;
    let (write, read) = stream.split();
    let mut connection = Connection { write, read };
    let (mut simulator, mut events) =
      simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
    simulator.connected = true;
    simulator.config.ws_url = Some(format!("ws://{address}/ocpp/CP-TEST"));
    simulator.security.pending_reconnect = Some(SecurityReconnectPlan {
      fallback_security_profile: SecurityProfileFallback::Restore(Some(1)),
    });
    let (commands, mut cmd_rx) = mpsc::unbounded_channel();
    let shutdown = matches!(command, SimulatorCommand::Shutdown);
    let reconnect = Box::pin(reconnect_after_security_change(
      &mut simulator,
      &mut connection,
      &mut cmd_rx,
    ));
    let cancel = async {
      let (socket, _) = listener.accept().await.expect("reconnect socket");
      commands.send(command).expect("cancel reconnect");
      socket
    };
    let (outcome, mut socket) =
      timeout(TEST_DEADLINE, async { tokio::join!(reconnect, cancel) })
        .await
        .expect("reconnect cancellation must be immediate");
    assert_eq!(matches!(outcome, OfflineOutcome::Exit), shutdown);
    assert!(!matches!(outcome, OfflineOutcome::Connect(_)));
    assert_eq!(simulator.config.security_profile, None);
    assert!(simulator.security.pending_reconnect.is_none());
    assert!(
      drain_log_messages(&mut events)
        .iter()
        .all(|message| !message.contains("Falling back"))
    );
    assert_connection_closed(&mut socket).await;
  }
}
