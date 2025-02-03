use ratatui::{
    style::{
        palette::tailwind::{BLUE, GREEN, PINK, RED, YELLOW},
        Style,
    },
    symbols,
    text::Line,
    widgets::{Block, Borders, Widget},
};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetState};

use super::style::LOG_BORDER_COLOR;

const STYLE_ERROR: Style = Style::new().fg(RED.c600);
const STYLE_WARN: Style = Style::new().fg(YELLOW.c400);
const STYLE_INFO: Style = Style::new().fg(BLUE.c400);
const STYLE_DEBUG: Style = Style::new().fg(GREEN.c600);
const STYLE_TRACE: Style = Style::new().fg(PINK.c600);

#[derive(Default)]
pub struct Log {
    state: TuiWidgetState,
}

impl Log {
    pub fn new() -> Self {
        Self {
            state: TuiWidgetState::new(),
        }
    }
}

impl Widget for &mut Log {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let block = Block::new()
            .title(Line::raw("Log"))
            .borders(Borders::all())
            .border_set(symbols::border::PLAIN)
            .border_style(LOG_BORDER_COLOR);

        TuiLoggerWidget::default()
            .block(block)
            .style_error(STYLE_ERROR)
            .style_warn(STYLE_WARN)
            .style_info(STYLE_INFO)
            .style_debug(STYLE_DEBUG)
            .style_trace(STYLE_TRACE)
            .output_separator(' ')
            .output_timestamp(Some("%H:%M:%S".to_string()))
            .output_level(Some(TuiLoggerLevelOutput::Long))
            .output_target(false)
            .output_file(false)
            .output_line(false)
            .state(&self.state)
            .render(area, buf);
    }
}
