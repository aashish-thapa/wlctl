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
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(vpn_block().inner(area));

    if modal.is_empty() {
        render_empty(frame, chunks[0]);
    } else {
        render_list(frame, chunks[0], modal);
    }

    frame.render_widget(hint(modal.is_empty()), chunks[1]);
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
            Row::new(vec![
                Cell::from(state_label(entry.state))
                    .style(Style::default().fg(state_color(entry.state)).bold()),
                Cell::from(Span::from(entry.info.id.clone()).bold()),
                Cell::from(entry.info.kind.to_string()).style(Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Fill(1),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .column_spacing(2)
        .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White));

    let mut state = TableState::default().with_selected(Some(modal.selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn hint(is_empty: bool) -> Paragraph<'static> {
    let spans = if is_empty {
        vec![Span::from("Esc").bold(), Span::from(" Close")]
    } else {
        vec![
            Span::from("↑↓").bold(),
            Span::from(" Move"),
            Span::from(" | "),
            Span::from("⏎").bold(),
            Span::from(" Toggle"),
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
