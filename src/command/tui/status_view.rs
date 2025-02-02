use ratatui::widgets::{Block, Paragraph, Widget};

#[derive(Debug, Default)]
pub struct StatusView {}

impl StatusView {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Widget for &mut StatusView {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let text = "Hello, Ratatui!\n\n\
                Created using https://github.com/ratatui/templates\n\
                Press `Esc`, `Ctrl-C` or `q` to stop running.";

        let paragraph = Paragraph::new(text).block(Block::bordered()).centered();

        paragraph.render(area, buf);
    }
}
