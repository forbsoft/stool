use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout},
    style::{
        palette::tailwind::{BLUE, SLATE},
        Color, Style, Stylize,
    },
    symbols,
    text::Line,
    widgets::{Block, Borders, Paragraph, Widget},
    DefaultTerminal, Frame,
};

use super::{create_backup_view::CreateBackupView, status_view::StatusView};

const HEADER_STYLE: Style = Style::new().fg(SLATE.c100).bg(BLUE.c800);
const HEADER_BG: Color = SLATE.c950;

#[derive(Debug, Default, PartialEq)]
enum View {
    #[default]
    Status,
    CreateBackup,
}

#[derive(Debug, Default)]
pub struct App<'a> {
    /// Is the application running?
    running: bool,

    view: View,

    create_backup_view: CreateBackupView<'a>,
    status_view: StatusView,
}

impl<'a> App<'a> {
    /// Construct a new instance of [`App`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the application's main loop.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;

        while self.running {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
            self.handle_crossterm_events()?;
        }

        Ok(())
    }

    /// Reads the crossterm events and updates the state of [`App`].
    ///
    /// If your application needs to perform work in between handling events, you can use the
    /// [`event::poll`] function to check if there are any events available with a timeout.
    fn handle_crossterm_events(&mut self) -> Result<()> {
        match event::read()? {
            // it's important to check KeyEventKind::Press to avoid handling key release events
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn on_key_event(&mut self, key: KeyEvent) {
        if self.view == View::CreateBackup {
            self.create_backup_view.on_key_event(key);
            return;
        }

        match (key.modifiers, key.code) {
            (_, KeyCode::Char('q')) => self.quit(),
            // Add other key handlers here.
            (_, KeyCode::Char('c')) => self.view = View::CreateBackup,
            (_, KeyCode::Esc) => self.view = View::Status,
            _ => {}
        }
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

impl<'a> Widget for &mut App<'a> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let [header_area, main_area, footer_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        //let title = Line::from(" S-Tool ").bold().blue().centered();

        let header = Block::new()
            .title(Line::raw("S-Tool").centered())
            .borders(Borders::TOP)
            .border_style(HEADER_STYLE)
            .border_set(symbols::border::EMPTY)
            .bg(HEADER_BG);

        let footer = Block::new()
            .title(Line::raw("FOOTER").centered())
            .borders(Borders::TOP)
            .border_style(HEADER_STYLE)
            .border_set(symbols::border::EMPTY)
            .bg(HEADER_BG);

        header.render(header_area, buf);

        match self.view {
            View::CreateBackup => self.create_backup_view.render(main_area, buf),
            View::Status => self.status_view.render(main_area, buf),
        }

        footer.render(footer_area, buf);
    }
}
