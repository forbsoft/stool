use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    symbols,
    text::Line,
    widgets::{Block, Borders, Widget},
};
use tui_textarea::TextArea;

use crate::engine::{self, BackupRequest};

#[derive(Debug)]
pub struct CreateBackupView<'a> {
    backup_tx: Sender<BackupRequest>,
    backup_name: TextArea<'a>,
    is_done: bool,
}

impl CreateBackupView<'_> {
    pub fn new(backup_tx: Sender<BackupRequest>) -> Self {
        let title = Line::raw("Create backup");

        let block = Block::default()
            .title(title)
            .border_set(symbols::border::ROUNDED)
            .border_style(Style::default())
            .borders(Borders::all());

        let mut backup_description = TextArea::default();
        backup_description.set_block(block);
        backup_description.set_cursor_line_style(Style::default());
        backup_description.set_placeholder_text("Enter backup name");

        Self {
            backup_tx,
            backup_name: backup_description,
            is_done: false,
        }
    }

    pub fn on_key_event(&mut self, event: KeyEvent) -> Result<(), anyhow::Error> {
        match event.code {
            KeyCode::Esc => self.is_done = true,
            KeyCode::Enter => {
                self.create_backup()?;
                return Ok(());
            }
            KeyCode::Down | KeyCode::Up => {}
            _ => {}
        }

        self.backup_name.input(event);

        Ok(())
    }

    pub fn is_done(&self) -> bool {
        self.is_done
    }

    pub fn create_backup(&mut self) -> Result<(), anyhow::Error> {
        if self.is_done {
            return Ok(());
        }

        self.is_done = true;

        let Some(description) = self.backup_name.lines().first().cloned() else {
            return Ok(());
        };

        if description.is_empty() {
            return Ok(());
        }

        let archive_name = engine::make_backup_filename(&description);

        self.backup_tx.send(BackupRequest::CreateBackup { archive_name })?;

        Ok(())
    }
}

impl Widget for &mut CreateBackupView<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let [backup_name_area, _] = Layout::vertical([Constraint::Length(3), Constraint::Length(10)]).areas(area);

        self.backup_name.render(backup_name_area, buf);
    }
}
