use crossterm::event::KeyEvent;
use ratatui::widgets::Widget;
use tui_textarea::TextArea;

#[derive(Debug, Default)]
pub struct CreateBackupView<'a> {
    backup_name: TextArea<'a>,
}

impl<'a> CreateBackupView<'a> {
    pub fn new() -> Self {
        let mut backup_name = TextArea::default();
        backup_name.set_placeholder_text("Enter backup name");

        Self { backup_name }
    }

    pub fn on_key_event(&mut self, event: KeyEvent) {
        self.backup_name.input(event);
    }
}

impl<'a> Widget for &mut CreateBackupView<'a> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        self.backup_name.render(area, buf);
    }
}
