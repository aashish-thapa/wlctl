use anyhow::Result;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

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

#[allow(dead_code)]
#[derive(Debug)]
pub struct EventHandler {
    pub sender: mpsc::UnboundedSender<Event>,
    pub receiver: mpsc::UnboundedReceiver<Event>,
    handler: tokio::task::JoinHandle<()>,
    tick_pending: Arc<AtomicBool>,
}

impl EventHandler {
    pub fn new(tick_rate: u64) -> Self {
        let tick_rate = Duration::from_millis(tick_rate);
        let (sender, receiver) = mpsc::unbounded_channel();
        let sender_cloned = sender.clone();
        let tick_pending = Arc::new(AtomicBool::new(false));
        let tick_pending_cloned = tick_pending.clone();
        let handler = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick = tokio::time::interval(tick_rate);
            loop {
                let tick_delay = tick.tick();
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                  () = sender_cloned.closed() => {
                    break;
                  }
                  _ = tick_delay => {
                    // A refresh can take longer than the timer interval. Keep
                    // at most one timer tick queued so the UI does not spend
                    // its time catching up with stale refresh requests.
                    if !tick_pending_cloned.swap(true, Ordering::AcqRel)
                        && sender_cloned.send(Event::Tick).is_err()
                    {
                        break;
                    }
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
            tick_pending,
        }
    }

    pub async fn next(&mut self) -> Result<Event> {
        self.receiver
            .recv()
            .await
            .ok_or(std::io::Error::other("This is an IO error").into())
    }

    /// Allow the timer to enqueue another refresh after the current one has
    /// fully completed.
    pub fn mark_tick_handled(&self) {
        self.tick_pending.store(false, Ordering::Release);
    }
}
