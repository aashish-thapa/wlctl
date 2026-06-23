use std::sync::atomic::Ordering;

use crate::nm::Mode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Clear, Paragraph},
};

use crate::app::{AdapterView, App, FocusedBlock};

pub fn render(app: &mut App, frame: &mut Frame) {
    if app.reset.enable {
        app.reset.render(frame);
    } else {
        let view = AdapterView {
            adapters: &app.adapters,
            active_index: app.active_index,
            selection_index: app.adapter_selection_index,
        };

        if !app.device.is_powered {
            app.device.render(
                frame,
                app.focused_block,
                app.config.clone(),
                &view,
                app.ethernet.as_ref(),
            )
        } else {
            let device = app.device.clone();
            match app.device.mode {
                Mode::Station => {
                    if let Some(station) = &mut app.device.station {
                        station.render(
                            frame,
                            app.focused_block,
                            &device,
                            app.config.clone(),
                            &view,
                        );
                    }
                }
                Mode::Ap => {
                    if let Some(ap) = &mut app.device.ap {
                        ap.render(frame, app.focused_block, &device, app.config.clone(), &view);
                    }
                }
            }
        };

        if app.focused_block == FocusedBlock::WpaEntrepriseAuth
            && let Some(eap) = &mut app.auth.eap
        {
            eap.render(frame);
        }

        if app.focused_block == FocusedBlock::AdapterInfos {
            app.adapter.render(frame, app.device.address.clone());
        }

        if app.focused_block == FocusedBlock::Doctor
            && let Some(modal) = &app.doctor
        {
            crate::doctor::render_modal(frame, modal);
        }

        if app.focused_block == FocusedBlock::Vpn
            && let Some(modal) = &app.vpn
        {
            crate::vpn::render_modal(frame, modal);
        }

        if app.focused_block == FocusedBlock::HiddenSsidInput {
            app.auth.hidden.render(frame);
        }

        if app.agent.psk_required.load(Ordering::Relaxed) {
            app.focused_block = FocusedBlock::PskAuthKey;

            app.auth
                .psk
                .render(frame, app.network_name_requiring_auth.clone());
        }

        if app
            .agent
            .private_key_passphrase_required
            .load(Ordering::Relaxed)
            && let Some(req) = &app.auth.request_key_passphrase
        {
            req.render(frame);
        }

        if app.agent.password_required.load(Ordering::Relaxed)
            && let Some(req) = &app.auth.request_password
        {
            req.render(frame);
        }

        if app
            .agent
            .username_and_password_required
            .load(Ordering::Relaxed)
            && let Some(req) = &app.auth.request_username_and_password
        {
            req.render(frame);
        }

        render_vpn_badge(frame, &app.active_vpns);

        // Notifications
        for (index, notification) in app.notifications.iter().enumerate() {
            notification.render(index, frame);
        }
    }
}

/// Draws a small always-on badge in the top-right when one or more VPN tunnels
/// are active, so the status is visible without opening the modal. The rect is
/// sized to the label's display width (profile names may contain wide chars).
fn render_vpn_badge(frame: &mut Frame, active_vpns: &[String]) {
    let Some(first) = active_vpns.first() else {
        return;
    };

    let extra = active_vpns.len().saturating_sub(1);
    let label = if extra > 0 {
        format!(" VPN: {first} +{extra} ")
    } else {
        format!(" VPN: {first} ")
    };

    let full = frame.area();
    let line = Line::from(label);
    let width = (line.width() as u16).min(full.width);
    if width == 0 {
        return;
    }

    let area = Rect {
        x: full.width.saturating_sub(width),
        y: 0,
        width,
        height: 1,
    };

    let badge =
        Paragraph::new(line).style(Style::default().fg(Color::Green).bg(Color::Black).bold());

    frame.render_widget(Clear, area);
    frame.render_widget(badge, area);
}
