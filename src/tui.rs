use crate::app::App;
use crate::event::EventHandler;
use crate::ui;
use anyhow::Result;
use ratatui::crossterm::{event::EnableMouseCapture, terminal::EnterAlternateScreen};
use ratatui::{
    Terminal,
    backend::Backend,
    crossterm::{
        event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste},
        terminal::{self, LeaveAlternateScreen},
    },
};
use std::io;
use std::panic;

#[derive(Debug)]
pub struct Tui<B: Backend> {
    terminal: Terminal<B>,
    pub events: EventHandler,
}

impl<B: Backend> Tui<B> {
    pub fn new(terminal: Terminal<B>, events: EventHandler) -> Self {
        Self { terminal, events }
    }

    pub fn init(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        ratatui::crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        let panic_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic| {
            Self::reset().expect("failed to reset the terminal");
            panic_hook(panic);
        }));

        self.terminal.hide_cursor()?;
        self.terminal.clear()?;
        Ok(())
    }

    pub fn draw(&mut self, app: &mut App) -> Result<()> {
        self.terminal.draw(|frame| ui::render(app, frame))?;
        Ok(())
    }

    fn reset() -> Result<()> {
        terminal::disable_raw_mode()?;
        // DisableBracketedPaste is harmless if it was never enabled; it ensures
        // the terminal isn't left in bracketed-paste mode after an import.
        ratatui::crossterm::execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        )?;
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        Self::reset()?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// Enables or disables terminal bracketed paste. Used to scope paste capture
    /// to text-input contexts (e.g. the VPN config import field) so a paste
    /// arrives as a single event instead of a flood of key presses.
    pub fn set_bracketed_paste(&self, enabled: bool) -> Result<()> {
        if enabled {
            ratatui::crossterm::execute!(io::stdout(), EnableBracketedPaste)?;
        } else {
            ratatui::crossterm::execute!(io::stdout(), DisableBracketedPaste)?;
        }
        Ok(())
    }
}
