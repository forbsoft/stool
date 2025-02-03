use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout},
    widgets::{Block, HighlightSpacing, List, ListItem, ListState, Padding, StatefulWidget, Widget},
};

use super::{app::View, style::MENU_HIGHLIGHT_STYLE};

#[derive(Debug)]
pub struct MenuItem {
    pub description: String,
    pub view: View,
}

#[derive(Debug)]
pub struct MenuView {
    items: Vec<MenuItem>,
    list_state: ListState,

    choice: Option<View>,
}

impl MenuView {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self {
            items,
            list_state: ListState::default(),
            choice: None,
        }
    }

    pub fn on_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Down => self.list_state.select_next(),
            KeyCode::Up => self.list_state.select_previous(),
            KeyCode::Enter => {
                let Some(ix) = self.list_state.selected() else {
                    return;
                };

                let Some(item) = self.items.get(ix) else {
                    return;
                };

                self.choice = Some(item.view);
            }
            _ => {}
        }
    }

    pub fn choice(&self) -> Option<View> {
        self.choice
    }

    pub fn clear(&mut self) {
        self.choice = None;
    }
}

impl Widget for &mut MenuView {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let block = Block::new().padding(Padding::top(1));

        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|item| ListItem::from(item.description.as_str()))
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(MENU_HIGHLIGHT_STYLE)
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);

        let [_, menu_area, _] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(20), Constraint::Fill(1)]).areas(area);

        // We need to disambiguate this trait method as both `Widget` and `StatefulWidget` share the
        // same method name `render`.
        StatefulWidget::render(list, menu_area, buf, &mut self.list_state);
    }
}
