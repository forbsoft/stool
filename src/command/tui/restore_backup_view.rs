use std::{fs, path::Path, sync::mpsc::Sender};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    style::Stylize,
    symbols,
    text::Line,
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState, StatefulWidget, Widget},
};

use crate::engine::BackupRequest;

use super::style::{list_item_color, LIST_BORDER_COLOR, LIST_HIGHLIGHT_STYLE};

#[derive(Debug)]
pub struct RestoreBackupView {
    backup_tx: Sender<BackupRequest>,

    items: Vec<String>,
    list_state: ListState,
    is_done: bool,
}

impl RestoreBackupView {
    pub fn new(backup_tx: Sender<BackupRequest>, backup_path: &Path) -> Result<Self, anyhow::Error> {
        let backup_files = fs::read_dir(backup_path)?;
        let mut backup_files: Vec<_> = backup_files
            .filter_map(Result::ok)
            .filter_map(|e| {
                let path = e.path();

                if !path.is_file() || !matches!(path.extension(), Some(ext) if ext == "7z") {
                    return None;
                }

                let metadata = path.metadata().unwrap();
                let modified = metadata.modified().unwrap();

                Some((path, modified))
            })
            .collect();

        backup_files.sort_by_key(|(_, v)| *v);
        backup_files.reverse();

        let items: Vec<_> = backup_files
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        Ok(Self {
            backup_tx,
            items,
            list_state: ListState::default(),
            is_done: false,
        })
    }

    pub fn on_key_event(&mut self, event: KeyEvent) -> Result<(), anyhow::Error> {
        match event.code {
            KeyCode::Esc => self.is_done = true,
            KeyCode::Down => self.list_state.select_next(),
            KeyCode::Up => self.list_state.select_previous(),
            KeyCode::PageDown => self.list_state.scroll_down_by(10),
            KeyCode::PageUp => self.list_state.scroll_up_by(10),
            KeyCode::Enter => {
                let Some(ix) = self.list_state.selected() else {
                    return Ok(());
                };

                let Some(item) = self.items.get(ix) else {
                    return Ok(());
                };

                self.restore_backup(item.to_owned())?;
            }
            _ => {}
        }

        Ok(())
    }

    pub fn is_done(&self) -> bool {
        self.is_done
    }

    pub fn restore_backup(&mut self, archive_name: String) -> Result<(), anyhow::Error> {
        if self.is_done {
            return Ok(());
        }

        self.is_done = true;

        self.backup_tx.send(BackupRequest::RestoreBackup { archive_name })?;

        Ok(())
    }
}

impl Widget for &mut RestoreBackupView {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let title = Line::raw("Restore backup");

        let block = Block::new()
            .title(title)
            .borders(Borders::all())
            .border_set(symbols::border::ROUNDED)
            .border_style(LIST_BORDER_COLOR);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let color = list_item_color(i);

                ListItem::from(item.as_str()).bg(color)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(LIST_HIGHLIGHT_STYLE)
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);

        // We need to disambiguate this trait method as both `Widget` and `StatefulWidget` share the
        // same method name `render`.
        StatefulWidget::render(list, area, buf, &mut self.list_state);
    }
}
