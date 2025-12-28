use std::process::Stdio;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use crate::event::Event;
use crate::notification::{Notification, NotificationLevel};

#[derive(Debug, Clone, Default)]
pub struct SpeedTest;

impl SpeedTest {
    pub async fn run(sender: UnboundedSender<Event>) {
        // Check if speedtest-cli is available
        let check = Command::new("which").arg("speedtest-cli").output().await;

        if check.is_err() || !check.unwrap().status.success() {
            let _ = Notification::send(
                "speedtest-cli not found. Install: pip install speedtest-cli".to_string(),
                NotificationLevel::Error,
                &sender,
            );
            return;
        }

        let _ = Notification::send(
            "Running speed test... (this may take ~30s)".to_string(),
            NotificationLevel::Info,
            &sender,
        );

        // Run speedtest-cli with simple output
        let result = Command::new("speedtest-cli")
            .arg("--simple")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match result {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Parse output format:
                    // Ping: 12.345 ms
                    // Download: 94.23 Mbit/s
                    // Upload: 45.67 Mbit/s
                    let mut ping = String::new();
                    let mut download = String::new();
                    let mut upload = String::new();

                    for line in stdout.lines() {
                        if line.starts_with("Ping:") {
                            ping = line.replace("Ping:", "").trim().to_string();
                        } else if line.starts_with("Download:") {
                            download = line.replace("Download:", "").trim().to_string();
                        } else if line.starts_with("Upload:") {
                            upload = line.replace("Upload:", "").trim().to_string();
                        }
                    }

                    let _ = Notification::send(
                        format!("↓ {} | ↑ {} | Ping: {}", download, upload, ping),
                        NotificationLevel::Info,
                        &sender,
                    );
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let _ = Notification::send(
                        format!("Speed test failed: {}", stderr.trim()),
                        NotificationLevel::Error,
                        &sender,
                    );
                }
            }
            Err(e) => {
                let _ = Notification::send(
                    format!("Failed to run speed test: {}", e),
                    NotificationLevel::Error,
                    &sender,
                );
            }
        }
    }
}
