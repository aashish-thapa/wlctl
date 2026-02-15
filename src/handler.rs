use anyhow::Result;
use std::sync::Arc;

use crate::app::{App, FocusedBlock};
use crate::config::Config;
use crate::device::Device;
use crate::event::Event;
use crate::mode::ap::APFocusedSection;
use crate::mode::station::share::Share;
use crate::mode::station::speed_test::SpeedTest;
use crate::nm::{Mode, SecurityType};
use crate::notification::{self, Notification};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;
use tui_input::backend::crossterm::EventHandler;

pub async fn toggle_connect(app: &mut App, sender: UnboundedSender<Event>) -> Result<()> {
    if let Some(station) = &mut app.device.station {
        match app.focused_block {
            FocusedBlock::NewNetworks => {
                if let Some(net_index) = station.new_networks_state.selected() {
                    if net_index < station.new_networks.len() {
                        let (net, _) = station.new_networks[net_index].clone();

                        // Check if it's an enterprise network
                        if net.network_type == SecurityType::Enterprise {
                            sender.send(Event::ConfigureNewEapNetwork(net.name.clone()))?;
                            return Ok(());
                        }

                        // Check if password is required for this network
                        if net.requires_password() {
                            // Request password from user
                            app.network_name_requiring_auth = Some(net.name.clone());
                            app.network_pending_auth = Some(net);
                            app.agent.request_passphrase(
                                app.network_name_requiring_auth.clone().unwrap(),
                            )?;
                            app.focused_block = FocusedBlock::PskAuthKey;
                            return Ok(());
                        }

                        // Open network - connect directly
                        tokio::spawn(async move {
                            let _ = net.connect(sender.clone(), None).await;
                        });
                    } else {
                        // Hidden network selected
                        let net = station.new_hidden_networks
                            [net_index.saturating_sub(station.new_networks.len())]
                        .clone();

                        if net.network_type == "8021x" {
                            sender.send(Event::ConfigureNewEapNetwork(net.address.clone()))?;
                            return Ok(());
                        }

                        // Hidden network connection - notify user it's not yet implemented
                        let _ = Notification::send(
                            "Hidden network connection not yet implemented".to_string(),
                            notification::NotificationLevel::Info,
                            &sender,
                        );
                    }
                }
            }
            FocusedBlock::KnownNetworks => match &station.connected_network {
                Some(connected_net) => {
                    if let Some(selected_net_index) = station.known_networks_state.selected() {
                        if selected_net_index > station.known_networks.len() - 1 {
                            // Can not connect to unavailble network
                            return Ok(());
                        }

                        let (selected_net, _signal) = &station.known_networks[selected_net_index];

                        if selected_net.name == connected_net.name {
                            station.disconnect(sender.clone()).await?;
                        } else {
                            let net_index = station
                                .known_networks
                                .iter()
                                .position(|(n, _s)| n.name == selected_net.name);

                            if let Some(index) = net_index {
                                let (net, _) = station.known_networks[index].clone();
                                station.disconnect(sender.clone()).await?;
                                tokio::spawn(async move {
                                    // Known networks already have saved credentials
                                    let _ = net.connect(sender.clone(), None).await;
                                });
                            }
                        }
                    }
                }
                None => {
                    if let Some(selected_net_index) = station.known_networks_state.selected() {
                        if selected_net_index > station.known_networks.len() - 1 {
                            // Can not connect to unavailble network
                            return Ok(());
                        }
                        let (selected_net, _signal) = &station.known_networks[selected_net_index];
                        let net_index = station
                            .known_networks
                            .iter()
                            .position(|(n, _s)| n.name == selected_net.name);

                        if let Some(index) = net_index {
                            let (net, _) = station.known_networks[index].clone();
                            tokio::spawn(async move {
                                // Known networks already have saved credentials
                                let _ = net.connect(sender.clone(), None).await;
                            });
                        }
                    }
                }
            },
            _ => {}
        }
    }
    Ok(())
}

async fn toggle_device_power(sender: UnboundedSender<Event>, device: &Device) -> Result<()> {
    if device.is_powered {
        match device.power_off().await {
            Ok(()) => {
                Notification::send(
                    "Device Powered Off".to_string(),
                    crate::notification::NotificationLevel::Info,
                    &sender.clone(),
                )?;
            }
            Err(e) => {
                Notification::send(
                    e.to_string(),
                    crate::notification::NotificationLevel::Error,
                    &sender.clone(),
                )?;
            }
        }
    } else {
        match device.power_on().await {
            Ok(()) => {
                Notification::send(
                    "Device Powered On".to_string(),
                    crate::notification::NotificationLevel::Info,
                    &sender.clone(),
                )?;
            }
            Err(e) => {
                Notification::send(
                    e.to_string(),
                    crate::notification::NotificationLevel::Error,
                    &sender.clone(),
                )?;
            }
        }
    }
    Ok(())
}

pub async fn handle_key_events(
    key_event: KeyEvent,
    app: &mut App,
    sender: UnboundedSender<Event>,
    config: Arc<Config>,
) -> Result<()> {
    if app.reset.enable {
        match key_event.code {
            KeyCode::Char('q') => {
                app.quit();
            }
            KeyCode::Esc if app.config.esc_quit => {
                app.quit();
            }
            KeyCode::Char('c' | 'C') => {
                if key_event.modifiers == KeyModifiers::CONTROL {
                    app.quit();
                }
            }

            KeyCode::Char('j') | KeyCode::Down => {
                if app.reset.selected_mode == Mode::Station {
                    app.reset.selected_mode = Mode::Ap;
                }
            }

            KeyCode::Char('k') | KeyCode::Up => {
                if app.reset.selected_mode == Mode::Ap {
                    app.reset.selected_mode = Mode::Station;
                }
            }

            KeyCode::Enter => {
                sender.send(Event::Reset(app.reset.selected_mode))?;
            }

            _ => {}
        }
        return Ok(());
    }

    if !app.device.is_powered {
        match app.focused_block {
            FocusedBlock::AdapterInfos => {
                if key_event.code == KeyCode::Esc {
                    app.focused_block = FocusedBlock::Device;
                }
            }

            FocusedBlock::Device => match key_event.code {
                KeyCode::Char('q') => {
                    app.quit();
                }
                KeyCode::Esc if app.config.esc_quit => {
                    app.quit();
                }

                KeyCode::Char('c' | 'C') => {
                    if key_event.modifiers == KeyModifiers::CONTROL {
                        app.quit();
                    }
                }

                KeyCode::Char(c) if c == config.device.infos => {
                    app.focused_block = FocusedBlock::AdapterInfos;
                }
                KeyCode::Char(c) if c == config.device.toggle_power => {
                    toggle_device_power(sender, &app.device).await?;
                }
                _ => {}
            },
            _ => {}
        }

        return Ok(());
    }

    match app.device.mode {
        Mode::Station => {
            if let Some(station) = &mut app.device.station {
                match app.focused_block {
                    FocusedBlock::HiddenSsidInput => match key_event.code {
                        KeyCode::Enter => {
                            let ssid: String = app.auth.hidden.ssid.value().into();
                            if !ssid.is_empty() {
                                let security = app.auth.hidden.security;
                                let password: Option<String> =
                                    if app.auth.hidden.requires_password() {
                                        Some(app.auth.hidden.password.value().into())
                                    } else {
                                        None
                                    };

                                let station_client = station.client.clone();
                                let device_path = station.device_path.clone();
                                let sender_clone = sender.clone();
                                app.auth.hidden.reset();
                                app.focused_block = FocusedBlock::NewNetworks;
                                tokio::spawn(async move {
                                    let _ = station_client
                                        .add_and_activate_hidden_connection(
                                            &device_path,
                                            &ssid,
                                            security,
                                            password.as_deref(),
                                        )
                                        .await
                                        .map(|_| {
                                            let _ = Notification::send(
                                                format!("Connecting to hidden network: {}", ssid),
                                                notification::NotificationLevel::Info,
                                                &sender_clone,
                                            );
                                        })
                                        .map_err(|e| {
                                            let _ = Notification::send(
                                                format!("Failed to connect to {}: {}", ssid, e),
                                                notification::NotificationLevel::Error,
                                                &sender_clone,
                                            );
                                        });
                                });
                            }
                        }

                        KeyCode::Tab => {
                            app.auth.hidden.next_field();
                        }

                        KeyCode::BackTab => {
                            app.auth.hidden.prev_field();
                        }

                        KeyCode::Left | KeyCode::Right => {
                            if app.auth.hidden.focused_field
                                == crate::mode::station::auth::hidden::HiddenField::Security
                            {
                                app.auth.hidden.cycle_security();
                                // If switched to Open while on Password field, move back
                            }
                        }

                        KeyCode::Esc => {
                            app.auth.hidden.reset();
                            app.focused_block = FocusedBlock::NewNetworks;
                        }

                        KeyCode::Char('h') if key_event.modifiers == KeyModifiers::CONTROL => {
                            app.auth.hidden.show_password = !app.auth.hidden.show_password;
                        }

                        _ => match app.auth.hidden.focused_field {
                            crate::mode::station::auth::hidden::HiddenField::Ssid => {
                                app.auth
                                    .hidden
                                    .ssid
                                    .handle_event(&crossterm::event::Event::Key(key_event));
                            }
                            crate::mode::station::auth::hidden::HiddenField::Password => {
                                app.auth
                                    .hidden
                                    .password
                                    .handle_event(&crossterm::event::Event::Key(key_event));
                            }
                            _ => {}
                        },
                    },

                    FocusedBlock::PskAuthKey => match key_event.code {
                        KeyCode::Enter => {
                            // Get the password before submit() resets it
                            let password: String = app.auth.psk.passphrase.value().into();
                            app.auth.psk.submit(&app.agent).await?;

                            // Connect to the pending network with the password
                            if let Some(net) = app.network_pending_auth.take() {
                                let sender_clone = sender.clone();
                                tokio::spawn(async move {
                                    let _ = net.connect(sender_clone, Some(&password)).await;
                                });
                            }

                            app.network_name_requiring_auth = None;
                            app.focused_block = FocusedBlock::NewNetworks;
                        }

                        KeyCode::Esc => {
                            app.auth.psk.cancel(&app.agent).await?;
                            app.network_pending_auth = None;
                            app.network_name_requiring_auth = None;
                            app.focused_block = FocusedBlock::NewNetworks;
                        }

                        KeyCode::Tab => {
                            app.auth.psk.show_password = !app.auth.psk.show_password;
                        }

                        _ => {
                            app.auth
                                .psk
                                .passphrase
                                .handle_event(&crossterm::event::Event::Key(key_event));
                        }
                    },

                    FocusedBlock::RequestKeyPasshphrase => {
                        if let Some(req) = &mut app.auth.request_key_passphrase {
                            match key_event.code {
                                KeyCode::Enter => {
                                    req.submit(&app.agent).await?;
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }

                                KeyCode::Esc => {
                                    req.cancel(&app.agent).await?;
                                    app.auth.request_key_passphrase = None;
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }

                                KeyCode::Tab => {
                                    req.show_password = !req.show_password;
                                }

                                _ => {
                                    req.passphrase
                                        .handle_event(&crossterm::event::Event::Key(key_event));
                                }
                            }
                        }
                    }
                    FocusedBlock::RequestPassword => {
                        if let Some(req) = &mut app.auth.request_password {
                            match key_event.code {
                                KeyCode::Enter => {
                                    req.submit(&app.agent).await?;
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }

                                KeyCode::Esc => {
                                    req.cancel(&app.agent).await?;
                                    app.auth.request_password = None;
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }

                                KeyCode::Tab => {
                                    req.show_password = !req.show_password;
                                }

                                _ => {
                                    req.password
                                        .handle_event(&crossterm::event::Event::Key(key_event));
                                }
                            }
                        }
                    }
                    FocusedBlock::RequestUsernameAndPassword => {
                        if let Some(req) = &mut app.auth.request_username_and_password {
                            match key_event.code {
                                KeyCode::Enter => {
                                    req.submit(&app.agent).await?;
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }

                                KeyCode::Esc => {
                                    req.cancel(&app.agent).await?;
                                    app.auth.request_username_and_password = None;
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }

                                _ => {
                                    req.handle_key_events(key_event, sender).await?;
                                }
                            }
                        }
                    }

                    FocusedBlock::WpaEntrepriseAuth => match key_event.code {
                        KeyCode::Esc => {
                            app.focused_block = FocusedBlock::NewNetworks;
                            app.auth.eap = None;
                        }

                        _ => {
                            if let Some(eap) = &mut app.auth.eap {
                                eap.handle_key_events(key_event, sender);
                            }
                        }
                    },
                    FocusedBlock::AdapterInfos => {
                        if key_event.code == KeyCode::Esc {
                            app.focused_block = FocusedBlock::Device;
                        }
                    }
                    FocusedBlock::ShareNetwork => {
                        if key_event.code == KeyCode::Esc {
                            station.share = None;
                            app.focused_block = FocusedBlock::KnownNetworks;
                        }
                    }
                    FocusedBlock::SpeedTest => {
                        // Close speed test popup on Esc or any key when not running
                        if key_event.code == KeyCode::Esc
                            || station
                                .speed_test
                                .as_ref()
                                .map(|s| !s.is_running)
                                .unwrap_or(true)
                        {
                            station.speed_test = None;
                            app.focused_block = FocusedBlock::KnownNetworks;
                        }
                    }
                    _ => {
                        match key_event.code {
                            KeyCode::Char('q') => {
                                app.quit();
                            }
                            KeyCode::Esc if app.config.esc_quit => {
                                app.quit();
                            }

                            KeyCode::Char('c' | 'C') => {
                                if key_event.modifiers == KeyModifiers::CONTROL {
                                    app.quit();
                                }
                            }

                            // Switch mode
                            KeyCode::Char(c)
                                if c == config.switch
                                    && key_event.modifiers == KeyModifiers::CONTROL =>
                            {
                                app.reset.enable = true;
                            }

                            KeyCode::Tab => match app.focused_block {
                                FocusedBlock::Device => {
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }
                                FocusedBlock::KnownNetworks => {
                                    app.focused_block = FocusedBlock::NewNetworks;
                                }
                                FocusedBlock::NewNetworks => {
                                    app.focused_block = FocusedBlock::Device;
                                }
                                _ => {}
                            },
                            KeyCode::BackTab => match app.focused_block {
                                FocusedBlock::Device => {
                                    app.focused_block = FocusedBlock::NewNetworks;
                                }
                                FocusedBlock::NewNetworks => {
                                    app.focused_block = FocusedBlock::KnownNetworks;
                                }
                                FocusedBlock::KnownNetworks => {
                                    app.focused_block = FocusedBlock::Device;
                                }
                                _ => {}
                            },

                            KeyCode::Char(c) if c == config.station.start_scanning => {
                                station.scan(sender).await?;
                            }
                            _ => match app.focused_block {
                                FocusedBlock::Device => match key_event.code {
                                    KeyCode::Char(c) if c == config.device.infos => {
                                        app.focused_block = FocusedBlock::AdapterInfos;
                                    }
                                    KeyCode::Char(c) if c == config.device.toggle_power => {
                                        toggle_device_power(sender, &app.device).await?;
                                    }
                                    _ => {}
                                },

                                FocusedBlock::KnownNetworks => {
                                    match key_event.code {
                                        // Share
                                        KeyCode::Char(c)
                                            if c == config.station.known_network.share =>
                                        {
                                            if let Some(net_index) =
                                                station.known_networks_state.selected()
                                            {
                                                if net_index > station.known_networks.len() - 1 {
                                                    let index = net_index.saturating_sub(
                                                        station.known_networks.len(),
                                                    );
                                                    let network =
                                                        &station.unavailable_known_networks[index];
                                                    // Check if it's a PSK network (WPA/WPA2/WPA3)
                                                    if matches!(
                                                        network.network_type,
                                                        SecurityType::WPA
                                                            | SecurityType::WPA2
                                                            | SecurityType::WPA3
                                                    ) && let Ok(share) = Share::new(
                                                        network.client.clone(),
                                                        &network.connection_path,
                                                        network.name.clone(),
                                                    )
                                                    .await
                                                    {
                                                        station.share = Some(share);
                                                        app.focused_block =
                                                            FocusedBlock::ShareNetwork;
                                                    }
                                                } else {
                                                    let (network, _) =
                                                        &station.known_networks[net_index];
                                                    // Check if it's a PSK network (WPA/WPA2/WPA3)
                                                    if matches!(
                                                        network.network_type,
                                                        SecurityType::WPA
                                                            | SecurityType::WPA2
                                                            | SecurityType::WPA3
                                                    ) && let Some(known) = &network.known_network
                                                        && let Ok(share) = Share::new(
                                                            known.client.clone(),
                                                            &known.connection_path,
                                                            network.name.clone(),
                                                        )
                                                        .await
                                                    {
                                                        station.share = Some(share);
                                                        app.focused_block =
                                                            FocusedBlock::ShareNetwork;
                                                    }
                                                }
                                            }
                                        }
                                        // Remove a known network
                                        KeyCode::Char(c)
                                            if c == config.station.known_network.remove =>
                                        {
                                            if let Some(net_index) =
                                                station.known_networks_state.selected()
                                            {
                                                if net_index > station.known_networks.len() - 1 {
                                                    let index = net_index.saturating_sub(
                                                        station.known_networks.len(),
                                                    );
                                                    let network =
                                                        &station.unavailable_known_networks[index];
                                                    network.forget(sender.clone()).await?;
                                                } else {
                                                    let (net, _signal) =
                                                        &station.known_networks[net_index];

                                                    if let Some(known_net) = &net.known_network {
                                                        known_net.forget(sender.clone()).await?;
                                                    }
                                                }
                                            }
                                        }

                                        // Toggle autoconnect
                                        KeyCode::Char(c)
                                            if c == config
                                                .station
                                                .known_network
                                                .toggle_autoconnect =>
                                        {
                                            if let Some(net_index) =
                                                station.known_networks_state.selected()
                                                && net_index < station.known_networks.len()
                                            {
                                                let (net, _) =
                                                    &mut station.known_networks[net_index];

                                                if let Some(known_net) = &mut net.known_network {
                                                    known_net
                                                        .toggle_autoconnect(sender.clone())
                                                        .await?;
                                                }
                                            }
                                        }

                                        // Show / Hide unavailable networks
                                        KeyCode::Char(c)
                                            if c == config.station.known_network.show_all =>
                                        {
                                            station.show_unavailable_known_networks =
                                                !station.show_unavailable_known_networks;
                                        }

                                        // Speed test
                                        KeyCode::Char(c)
                                            if c == config.station.known_network.speed_test =>
                                        {
                                            // Only run speed test if connected
                                            if station.connected_network.is_some()
                                                || station.is_ethernet_connected
                                            {
                                                // Show loading popup
                                                station.speed_test = Some(SpeedTest::new());
                                                app.focused_block = FocusedBlock::SpeedTest;

                                                // Run speed test in background
                                                let sender_clone = sender.clone();
                                                tokio::spawn(async move {
                                                    let result =
                                                        SpeedTest::run(sender_clone.clone()).await;
                                                    let _ = sender_clone
                                                        .send(Event::SpeedTestResult(result));
                                                });
                                            } else {
                                                Notification::send(
                                                    "Not connected to any network".to_string(),
                                                    notification::NotificationLevel::Warning,
                                                    &sender,
                                                )?;
                                            }
                                        }

                                        // Connect/Disconnect
                                        KeyCode::Enter | KeyCode::Char(' ') => {
                                            toggle_connect(app, sender).await?
                                        }

                                        // Scroll down
                                        KeyCode::Char('j') | KeyCode::Down => {
                                            if !station.known_networks.is_empty() {
                                                let i =
                                                    match station.known_networks_state.selected() {
                                                        Some(i) => {
                                                            let limit = if station
                                                                .show_unavailable_known_networks
                                                            {
                                                                station.known_networks.len()
                                                                    + station
                                                                        .unavailable_known_networks
                                                                        .len()
                                                                    - 1
                                                            } else {
                                                                station.known_networks.len() - 1
                                                            };

                                                            if i < limit { i + 1 } else { i }
                                                        }
                                                        None => 0,
                                                    };

                                                station.known_networks_state.select(Some(i));
                                            }
                                        }
                                        KeyCode::Char('k') | KeyCode::Up => {
                                            if !station.known_networks.is_empty() {
                                                let i =
                                                    match station.known_networks_state.selected() {
                                                        Some(i) => i.saturating_sub(1),
                                                        None => 0,
                                                    };

                                                station.known_networks_state.select(Some(i));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                FocusedBlock::NewNetworks => match key_event.code {
                                    // Show / Hide unavailable networks
                                    KeyCode::Char(c)
                                        if c == config.station.new_network.show_all =>
                                    {
                                        station.show_hidden_networks =
                                            !station.show_hidden_networks;
                                    }
                                    // Connect to hidden network
                                    KeyCode::Char(c)
                                        if c == config.station.new_network.connect_hidden =>
                                    {
                                        app.focused_block = FocusedBlock::HiddenSsidInput;
                                    }
                                    KeyCode::Enter | KeyCode::Char(' ') => {
                                        toggle_connect(app, sender).await?
                                    }
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        if !station.new_networks.is_empty() {
                                            let i = match station.new_networks_state.selected() {
                                                Some(i) => {
                                                    let limit = if station.show_hidden_networks {
                                                        station.new_networks.len()
                                                            + station.new_hidden_networks.len()
                                                            - 1
                                                    } else {
                                                        station.new_networks.len() - 1
                                                    };
                                                    if i < limit { i + 1 } else { i }
                                                }
                                                None => 0,
                                            };

                                            station.new_networks_state.select(Some(i));
                                        }
                                    }
                                    KeyCode::Char('k') | KeyCode::Up => {
                                        if !station.new_networks.is_empty() {
                                            let i = match station.new_networks_state.selected() {
                                                Some(i) => i.saturating_sub(1),
                                                None => 0,
                                            };

                                            station.new_networks_state.select(Some(i));
                                        }
                                    }
                                    _ => {}
                                },
                                _ => {}
                            },
                        }
                    }
                }
            } else {
                sender.send(Event::Reset(Mode::Station))?;
            }
        }

        Mode::Ap => {
            if let Some(ap) = &mut app.device.ap {
                match app.focused_block {
                    FocusedBlock::AccessPointInput => match key_event.code {
                        KeyCode::Enter => {
                            ap.start(sender.clone()).await?;
                            app.focused_block = FocusedBlock::Device;
                        }

                        KeyCode::Esc => {
                            ap.ap_start
                                .store(false, std::sync::atomic::Ordering::Relaxed);
                            app.focused_block = FocusedBlock::AccessPoint;
                        }
                        KeyCode::Tab => match ap.focused_section {
                            APFocusedSection::SSID => {
                                ap.focused_section = APFocusedSection::PSK;
                            }
                            APFocusedSection::PSK => {
                                ap.focused_section = APFocusedSection::SSID;
                            }
                        },
                        _ => match ap.focused_section {
                            APFocusedSection::SSID => {
                                ap.ssid
                                    .handle_event(&crossterm::event::Event::Key(key_event));
                            }
                            APFocusedSection::PSK => {
                                ap.psk
                                    .handle_event(&crossterm::event::Event::Key(key_event));
                            }
                        },
                    },

                    FocusedBlock::AdapterInfos => {
                        if key_event.code == KeyCode::Esc {
                            app.focused_block = FocusedBlock::Device;
                        }
                    }
                    _ => {
                        match key_event.code {
                            KeyCode::Char('q') => {
                                app.quit();
                            }
                            KeyCode::Esc if app.config.esc_quit => {
                                app.quit();
                            }

                            KeyCode::Char('c' | 'C') => {
                                if key_event.modifiers == KeyModifiers::CONTROL {
                                    app.quit();
                                }
                            }

                            // Switch mode
                            KeyCode::Char(c)
                                if c == config.switch
                                    && key_event.modifiers == KeyModifiers::CONTROL =>
                            {
                                app.reset.enable = true;
                            }

                            KeyCode::Tab => match app.focused_block {
                                FocusedBlock::Device => {
                                    app.focused_block = FocusedBlock::AccessPoint;
                                }
                                FocusedBlock::AccessPoint => {
                                    if ap.connected_devices.is_empty() {
                                        app.focused_block = FocusedBlock::Device;
                                    } else {
                                        app.focused_block =
                                            FocusedBlock::AccessPointConnectedDevices;
                                    }
                                }
                                FocusedBlock::AccessPointConnectedDevices => {
                                    app.focused_block = FocusedBlock::Device;
                                }

                                _ => {}
                            },

                            _ => {
                                if app.focused_block == FocusedBlock::Device {
                                    match key_event.code {
                                        KeyCode::Char(c) if c == config.device.infos => {
                                            app.focused_block = FocusedBlock::AdapterInfos;
                                        }
                                        KeyCode::Char(c) if c == config.device.toggle_power => {
                                            toggle_device_power(sender, &app.device).await?;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                sender.send(Event::Reset(Mode::Ap))?;
            }
        }
    }

    Ok(())
}
