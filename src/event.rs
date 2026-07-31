use anyhow::Result;

use crate::nm::Mode;
use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseEvent};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;

use crate::doctor::CheckEntry;
use crate::mode::station::speed_test::SpeedTest;
use crate::notification::Notification;

#[derive(Clone, Debug)]
pub enum Event {
    Tick,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
    Notification(Notification),
    Reset(Mode),
    Auth(String),
    EapNeworkConfigured(String),
    ConfigureNewEapNetwork(String),
    AuthRequestPassword((String, Option<String>)),
    AuthReqKeyPassphrase(String),
    AuthReqUsernameAndPassword(String),
    UsernameAndPasswordSubmit,
    SpeedTestResult(SpeedTest),
    DoctorCompleted {
        run_id: u64,
        results: Vec<CheckEntry>,
    },
}

/// Forwards terminal input into the main loop.
///
/// Periodic refreshes are not produced here: the main loop drives them from its
/// own timer so a refresh can never be queued while one is already running.
#[allow(dead_code)]
#[derive(Debug)]
pub struct EventHandler {
    pub sender: mpsc::UnboundedSender<Event>,
    pub receiver: mpsc::UnboundedReceiver<Event>,
    handler: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let sender_cloned = sender.clone();
        let handler = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            loop {
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                  () = sender_cloned.closed() => {
                    break;
                  }
                  Some(Ok(evt)) = crossterm_event => {
                    match evt {
                      CrosstermEvent::Key(key)
                        if key.kind == crossterm::event::KeyEventKind::Press =>
                      {
                        sender_cloned.send(Event::Key(key)).unwrap();
                      },
                      CrosstermEvent::Resize(x, y) => {
                        sender_cloned.send(Event::Resize(x, y)).unwrap();
                      },
                      // Only emitted while bracketed paste is enabled (scoped to
                      // the VPN config import field).
                      CrosstermEvent::Paste(text) => {
                        sender_cloned.send(Event::Paste(text)).unwrap();
                      },
                      _ => {}
                    }
                  }
                };
            }
        });
        Self {
            sender,
            receiver,
            handler,
        }
    }

    /// Waits for the next terminal event.
    ///
    /// Cancel-safe: the underlying receive drops without losing a message, so
    /// this can be raced against a timer in `select!`.
    pub async fn next(&mut self) -> Result<Event> {
        self.receiver
            .recv()
            .await
            .ok_or(std::io::Error::other("This is an IO error").into())
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}
