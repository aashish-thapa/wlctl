use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Padding, Paragraph, Row, Table, TableState,
    },
};

use super::VpnModal;
use crate::nm::ActiveConnectionState;

/// Draws the VPN modal centered on top of the current frame.
pub fn render_modal(frame: &mut Frame, modal: &VpnModal) {
    let area = popup_area(frame.area());

    frame.render_widget(Clear, area);
    frame.render_widget(vpn_block(), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(vpn_block().inner(area));

    if modal.is_empty() {
        render_empty(frame, chunks[0]);
    } else {
        render_list(frame, chunks[0], modal);
    }

    frame.render_widget(detail(modal), chunks[1]);
    frame.render_widget(hint(modal), chunks[2]);
}

/// One-line detail for the selected entry: its assigned IPv4 and uptime while
/// up. Doubles as the prompt label while importing. Blank when nothing is
/// selected or the tunnel is down.
fn detail(modal: &VpnModal) -> Paragraph<'static> {
    if modal.import_input.is_some() {
        return Paragraph::new("Path to a WireGuard .conf  —  Enter to import, Esc to cancel")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
    }

    let text = modal
        .selected_entry()
        .filter(|e| e.is_active())
        .map(|e| {
            let mut parts = Vec::new();
            if let Some(ip) = &e.ipv4 {
                parts.push(ip.clone());
            }
            if let Some(up) = e.uptime() {
                parts.push(up);
            }
            parts.join("  ·  ")
        })
        .unwrap_or_default();

    Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray))
}

fn popup_area(full: Rect) -> Rect {
    let modal_h = full.height.saturating_sub(4).clamp(8, 18);
    let modal_w = full.width.saturating_sub(4).clamp(40, 64);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(modal_h),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .split(full);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(modal_w),
            Constraint::Fill(1),
        ])
        .split(vertical[1])[1]
}

fn vpn_block() -> Block<'static> {
    Block::default()
        .title(" VPN ")
        .title_alignment(Alignment::Center)
        .title_style(Style::default().bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .border_type(BorderType::Thick)
        .padding(Padding::uniform(1))
}

fn render_empty(frame: &mut Frame, area: Rect) {
    let p = Paragraph::new(vec![
        Line::from("No VPN connections configured.").centered(),
        Line::from(""),
        Line::from("Add one with nmtui or your desktop's network settings.")
            .centered()
            .style(Style::default().fg(Color::DarkGray)),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(p, area);
}

fn render_list(frame: &mut Frame, area: Rect, modal: &VpnModal) {
    let rows: Vec<Row> = modal
        .entries
        .iter()
        .map(|entry| {
            let (auto_label, auto_color) = if entry.info.autoconnect {
                ("✓", Color::Green)
            } else {
                ("·", Color::DarkGray)
            };
            Row::new(vec![
                Cell::from(state_label(entry.state))
                    .style(Style::default().fg(state_color(entry.state)).bold()),
                Cell::from(Span::from(entry.info.id.clone()).bold()),
                Cell::from(entry.info.kind.to_string()).style(Style::default().fg(Color::DarkGray)),
                Cell::from(Line::from(auto_label).centered())
                    .style(Style::default().fg(auto_color)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(7),
        Constraint::Fill(1),
        Constraint::Length(10),
        Constraint::Length(4),
    ];

    let header = Row::new(vec![
        Cell::from("STATE"),
        Cell::from("NAME"),
        Cell::from("TYPE"),
        Cell::from(Line::from("AUTO").centered()),
    ])
    .style(Style::default().fg(Color::DarkGray))
    .bottom_margin(1);

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(2)
        .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White));

    let mut state = TableState::default().with_selected(Some(modal.selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn hint(modal: &VpnModal) -> Paragraph<'static> {
    // While importing, the hint line is the path input field.
    if let Some(buf) = &modal.import_input {
        let line = Line::from(vec![
            Span::from("Path: ").bold(),
            Span::from(format!("{buf}_")),
        ]);
        return Paragraph::new(line)
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::White));
    }

    // A pending delete swaps the hint line for a confirmation prompt.
    if modal.pending_delete {
        let name = modal
            .selected_entry()
            .map(|e| e.info.id.clone())
            .unwrap_or_default();
        let line = Line::from(vec![
            Span::from(format!("Delete '{name}'?  ")),
            Span::from("y").bold(),
            Span::from(" Yes  "),
            Span::from("n").bold(),
            Span::from(" No"),
        ]);
        return Paragraph::new(line)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red));
    }

    let spans = if modal.is_empty() {
        vec![
            Span::from("i").bold(),
            Span::from(" Import"),
            Span::from(" | "),
            Span::from("Esc").bold(),
            Span::from(" Close"),
        ]
    } else {
        vec![
            Span::from("↑↓").bold(),
            Span::from(" Move"),
            Span::from(" | "),
            Span::from("⏎").bold(),
            Span::from(" Toggle"),
            Span::from(" | "),
            Span::from("a").bold(),
            Span::from(" Auto"),
            Span::from(" | "),
            Span::from("d").bold(),
            Span::from(" Delete"),
            Span::from(" | "),
            Span::from("i").bold(),
            Span::from(" Import"),
            Span::from(" | "),
            Span::from("Esc").bold(),
            Span::from(" Close"),
        ]
    };

    Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Blue))
}

fn state_label(state: ActiveConnectionState) -> &'static str {
    match state {
        ActiveConnectionState::Activated => "on",
        ActiveConnectionState::Activating => "…",
        ActiveConnectionState::Deactivating => "…",
        _ => "off",
    }
}

fn state_color(state: ActiveConnectionState) -> Color {
    match state {
        ActiveConnectionState::Activated => Color::Green,
        ActiveConnectionState::Activating | ActiveConnectionState::Deactivating => Color::Yellow,
        _ => Color::DarkGray,
    }
}
