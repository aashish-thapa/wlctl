use async_channel::{Receiver, Sender};
use std::sync::{Arc, atomic::AtomicBool};
use tokio::sync::mpsc::UnboundedSender;

use crate::event::Event;

/// Authentication agent for handling credential requests
///
/// In NetworkManager, unlike iwd, we don't need to implement a D-Bus agent interface.
/// Instead, credentials are collected from the user and passed to NetworkManager
/// when creating/activating connections. This agent struct provides the coordination
/// mechanism for the UI to collect and provide credentials.
#[derive(Debug, Clone)]
pub struct AuthAgent {
    pub tx_cancel: Sender<()>,
    pub rx_cancel: Receiver<()>,
    pub tx_passphrase: Sender<String>,
    pub rx_passphrase: Receiver<String>,
    pub tx_username_password: Sender<(String, String)>,
    pub rx_username_password: Receiver<(String, String)>,
    pub psk_required: Arc<AtomicBool>,
    pub private_key_passphrase_required: Arc<AtomicBool>,
    pub password_required: Arc<AtomicBool>,
    pub username_and_password_required: Arc<AtomicBool>,
    pub event_sender: UnboundedSender<Event>,
}

impl AuthAgent {
    pub fn new(sender: UnboundedSender<Event>) -> Self {
        let (tx_passphrase, rx_passphrase) = async_channel::unbounded();
        let (tx_username_password, rx_username_password) = async_channel::unbounded();
        let (tx_cancel, rx_cancel) = async_channel::unbounded();

        Self {
            tx_cancel,
            rx_cancel,
            tx_passphrase,
            rx_passphrase,
            tx_username_password,
            rx_username_password,
            psk_required: Arc::new(AtomicBool::new(false)),
            private_key_passphrase_required: Arc::new(AtomicBool::new(false)),
            password_required: Arc::new(AtomicBool::new(false)),
            username_and_password_required: Arc::new(AtomicBool::new(false)),
            event_sender: sender,
        }
    }

    /// Request PSK passphrase from user
    pub fn request_passphrase(&self, network_name: String) -> anyhow::Result<()> {
        self.psk_required
            .store(true, std::sync::atomic::Ordering::Relaxed);

        self.event_sender
            .send(Event::Auth(network_name))
            .map_err(|e| anyhow::anyhow!("Failed to send auth event: {}", e))?;

        Ok(())
    }

    /// Request private key passphrase from user
    pub fn request_private_key_passphrase(&self, network_name: String) -> anyhow::Result<()> {
        self.private_key_passphrase_required
            .store(true, std::sync::atomic::Ordering::Relaxed);

        self.event_sender
            .send(Event::AuthReqKeyPassphrase(network_name))
            .map_err(|e| anyhow::anyhow!("Failed to send auth event: {}", e))?;

        Ok(())
    }

    /// Request username and password from user
    pub fn request_username_and_password(&self, network_name: String) -> anyhow::Result<()> {
        self.username_and_password_required
            .store(true, std::sync::atomic::Ordering::Relaxed);

        self.event_sender
            .send(Event::AuthReqUsernameAndPassword(network_name))
            .map_err(|e| anyhow::anyhow!("Failed to send auth event: {}", e))?;

        Ok(())
    }

    /// Request password from user (with optional pre-filled username)
    pub fn request_password(
        &self,
        network_name: String,
        user_name: Option<String>,
    ) -> anyhow::Result<()> {
        self.password_required
            .store(true, std::sync::atomic::Ordering::Relaxed);

        self.event_sender
            .send(Event::AuthRequestPassword((network_name, user_name)))
            .map_err(|e| anyhow::anyhow!("Failed to send auth event: {}", e))?;

        Ok(())
    }

    /// Wait for passphrase response with cancellation support
    pub async fn wait_for_passphrase(&self) -> Option<String> {
        tokio::select! {
            r = self.rx_passphrase.recv() => {
                r.ok()
            }
            _ = self.rx_cancel.recv() => {
                None
            }
        }
    }

    /// Wait for username/password response with cancellation support
    pub async fn wait_for_username_password(&self) -> Option<(String, String)> {
        tokio::select! {
            r = self.rx_username_password.recv() => {
                match r {
                    Ok((username, password)) => Some((username, password)),
                    Err(_) => None,
                }
            }
            _ = self.rx_cancel.recv() => {
                None
            }
        }
    }

    /// Cancel any pending credential request
    pub async fn cancel(&self) {
        let _ = self.tx_cancel.send(()).await;
    }

    /// Reset all flags
    pub fn reset(&self) {
        self.psk_required
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.private_key_passphrase_required
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.password_required
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.username_and_password_required
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
