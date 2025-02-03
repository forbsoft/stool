use ratatui::style::{
    palette::tailwind::{BLUE, SLATE},
    Color, Style,
};

pub const HEADER_STYLE: Style = Style::new().fg(SLATE.c100).bg(BLUE.c800);

pub const LIST_BORDER_COLOR: Color = Color::Gray;
pub const LIST_ITEM_BG: Color = Color::Black;
pub const LIST_ITEM_ALT_BG: Color = Color::Rgb(16, 16, 16);
pub const LIST_HIGHLIGHT_STYLE: Style = Style::new().fg(Color::LightGreen);

pub const LOG_BORDER_COLOR: Color = Color::Gray;

pub const MENU_HIGHLIGHT_STYLE: Style = Style::new().fg(Color::LightGreen);

pub const PROGRESS_BAR_STYLE: Color = Color::Blue;
pub const PROGRESS_BAR_BG_COLOR: Color = Color::Rgb(20, 20, 20);

pub const FOOTER_AUTOBACKUP_ON_STYLE: Style = Style::new().bg(Color::Green);
pub const FOOTER_AUTOBACKUP_OFF_STYLE: Style = Style::new().bg(Color::Red);

pub const fn list_item_color(i: usize) -> Color {
    if i % 2 == 0 {
        LIST_ITEM_BG
    } else {
        LIST_ITEM_ALT_BG
    }
}
