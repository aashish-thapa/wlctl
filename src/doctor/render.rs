use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Padding, Paragraph, Row, Table},
};

use super::check::Status;
use super::{CheckEntry, DoctorModal};

/// Draws the doctor modal centered on top of the current frame.
pub fn render_modal(frame: &mut Frame, modal: &DoctorModal) {
    let area = popup_area(frame.area());

    frame.render_widget(Clear, area);

    match modal {
        DoctorModal::Running => render_running(frame, area),
        DoctorModal::Ready(entries) => render_results(frame, area, entries),
    }
}

fn popup_area(full: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(20),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .split(full);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Min(72),
            Constraint::Fill(1),
        ])
        .split(vertical[1])[1]
}

fn doctor_block() -> Block<'static> {
    Block::default()
        .title(" Doctor ")
        .title_alignment(Alignment::Center)
        .title_style(Style::default().bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .border_type(BorderType::Thick)
        .padding(Padding::uniform(1))
}

fn render_running(frame: &mut Frame, area: Rect) {
    let p = Paragraph::new(Line::from("Running diagnostics...").centered())
        .block(doctor_block())
        .alignment(Alignment::Center);
    frame.render_widget(p, area);
}

fn render_results(frame: &mut Frame, area: Rect, entries: &[CheckEntry]) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(doctor_block().inner(area));

    frame.render_widget(doctor_block(), area);

    let rows: Vec<Row> = entries
        .iter()
        .map(|(name, outcome)| {
            Row::new(vec![
                Cell::from(status_label(outcome.status))
                    .style(Style::default().fg(status_color(outcome.status)).bold()),
                Cell::from(Span::from(*name).bold()),
                Cell::from(outcome.summary.clone()),
            ])
        })
        .chain(verdict_rows(entries))
        .collect();

    let widths = [
        Constraint::Length(6),
        Constraint::Length(14),
        Constraint::Fill(1),
    ];

    frame.render_widget(Table::new(rows, widths).column_spacing(2), chunks[0]);

    let hint = Paragraph::new(Line::from(vec![
        Span::from("Esc").bold(),
        Span::from(" Close"),
    ]))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Blue));
    frame.render_widget(hint, chunks[1]);
}

fn verdict_rows(entries: &[CheckEntry]) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    let has_verdicts = entries.iter().any(|(_, o)| o.verdict.is_some());
    if !has_verdicts {
        return rows;
    }

    rows.push(Row::new(vec![Cell::from("")]));
    for (_, outcome) in entries {
        if let Some(verdict) = &outcome.verdict {
            rows.push(Row::new(vec![
                Cell::from("→").style(Style::default().fg(Color::Yellow).bold()),
                Cell::from(""),
                Cell::from(verdict.clone()).style(Style::default().fg(Color::Yellow)),
            ]));
        }
    }
    rows
}

fn status_label(status: Status) -> &'static str {
    match status {
        Status::Ok => "ok",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::Skip => "skip",
    }
}

fn status_color(status: Status) -> Color {
    match status {
        Status::Ok => Color::Green,
        Status::Warn => Color::Yellow,
        Status::Fail => Color::Red,
        Status::Skip => Color::DarkGray,
    }
}
