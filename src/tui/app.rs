use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::Stylize,
    symbols,
    text::Line,
    widgets::{Block, Borders, Gauge, Padding, Paragraph, Widget},
    DefaultTerminal,
};

use crate::engine::{Engine, EngineControl};

use super::{
    create_backup_view::CreateBackupView,
    log_widget::Log,
    menu_view::{MenuItem, MenuView},
    restore_backup_view::RestoreBackupView,
    state::AppState,
    style::{
        FOOTER_AUTOBACKUP_OFF_STYLE, FOOTER_AUTOBACKUP_ON_STYLE, HEADER_STYLE, PROGRESS_BAR_BG_COLOR,
        PROGRESS_BAR_STYLE,
    },
};

const EVENT_POLL_DURATION: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum View {
    #[default]
    Menu,
    CreateBackup,
    RestoreBackup,
    Shutdown,
}

pub struct App<'a> {
    state: Arc<Mutex<AppState>>,
    backup_path: PathBuf,
    engine: Engine,
    engine_control: EngineControl,
    shutdown: Arc<AtomicBool>,

    view: View,

    log_widget: Log,
    menu_view: MenuView,
    create_backup_view: Option<CreateBackupView<'a>>,
    restore_backup_view: Option<RestoreBackupView>,
}

impl App<'_> {
    pub fn new(state: Arc<Mutex<AppState>>, engine: Engine, backup_path: PathBuf, shutdown: Arc<AtomicBool>) -> Self {
        let engine_control = engine.control();

        Self {
            state,
            engine,
            engine_control,
            backup_path,
            shutdown,

            view: View::Menu,

            log_widget: Log::default(),

            menu_view: MenuView::new(vec![
                MenuItem {
                    description: "Create backup".to_owned(),
                    view: View::CreateBackup,
                },
                MenuItem {
                    description: "Restore backup".to_owned(),
                    view: View::RestoreBackup,
                },
                MenuItem {
                    description: "Exit".to_owned(),
                    view: View::Shutdown,
                },
            ]),

            create_backup_view: None,
            restore_backup_view: None,
        }
    }

    /// Run the application's main loop.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<(), anyhow::Error> {
        let mut shutting_down = false;

        loop {
            if !shutting_down {
                if self.shutdown.load(Ordering::Relaxed) {
                    self.view = View::Shutdown;
                }

                if self.view == View::Shutdown {
                    // Request engine shutdown
                    self.engine_control.shutdown();

                    shutting_down = true;
                }
            } else if shutting_down && self.engine.has_shut_down() {
                break;
            }

            self.create_views()?;

            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;

            if crossterm::event::poll(EVENT_POLL_DURATION)? {
                self.handle_crossterm_events()?;
            };
        }

        // Wait for engine thread to finish
        self.engine.join();

        Ok(())
    }

    /// Reads the crossterm events and updates the state of [`App`].
    fn handle_crossterm_events(&mut self) -> Result<(), anyhow::Error> {
        if !event::poll(EVENT_POLL_DURATION)? {
            return Ok(());
        }

        match event::read()? {
            // it's important to check KeyEventKind::Press to avoid handling key release events
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key)?,
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }

        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn on_key_event(&mut self, key: KeyEvent) -> Result<(), anyhow::Error> {
        'view: {
            match self.view {
                View::CreateBackup => {
                    let Some(view) = self.create_backup_view.as_mut() else {
                        break 'view;
                    };

                    view.on_key_event(key)?;

                    if view.is_done() {
                        self.view = View::Menu;
                        self.create_backup_view = None;
                    }

                    return Ok(());
                }
                View::RestoreBackup => {
                    let Some(view) = self.restore_backup_view.as_mut() else {
                        break 'view;
                    };

                    view.on_key_event(key)?;

                    if view.is_done() {
                        self.view = View::Menu;
                        self.restore_backup_view = None;
                    }

                    return Ok(());
                }
                View::Shutdown => return Ok(()),
                _ => {}
            }
        }

        match (key.modifiers, key.code) {
            (_, KeyCode::Char('q')) => self.quit(),
            // F12 to toggle Autobackup
            (_, KeyCode::F(12)) => {
                let control = self.engine.control();

                control.set_autobackup(!control.get_autobackup())
            }
            _ => {
                self.menu_view.on_key_event(key);

                if let Some(view) = self.menu_view.choice() {
                    self.menu_view.clear();

                    self.view = view;
                }
            }
        }

        if self.view == View::Shutdown {
            self.quit();
        }

        Ok(())
    }

    /// Create views if needed
    fn create_views(&mut self) -> Result<(), anyhow::Error> {
        if self.view == View::CreateBackup && self.create_backup_view.is_none() {
            self.create_backup_view = Some(CreateBackupView::new(self.engine_control.clone()));
        }

        if self.view == View::RestoreBackup && self.restore_backup_view.is_none() {
            self.restore_backup_view = Some(RestoreBackupView::new(self.engine_control.clone(), &self.backup_path)?);
        }

        Ok(())
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.view = View::Shutdown;
    }
}

impl Widget for &mut App<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let [header_area, main_area, footer_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(area);

        let [main_area, log_area] = Layout::vertical([Constraint::Fill(1), Constraint::Length(10)]).areas(main_area);

        let header = Block::new()
            .title(Line::raw("S-Tool").centered())
            .borders(Borders::TOP)
            .border_style(HEADER_STYLE)
            .border_set(symbols::border::EMPTY);

        header.render(header_area, buf);

        match self.view {
            View::CreateBackup => {
                if let Some(view) = self.create_backup_view.as_mut() {
                    view.render(main_area, buf);
                }
            }
            View::RestoreBackup => {
                if let Some(view) = self.restore_backup_view.as_mut() {
                    view.render(main_area, buf);
                }
            }
            View::Shutdown => {
                let block = Block::new().padding(Padding::top(1));

                Paragraph::new("Shutting down...")
                    .block(block)
                    .bold()
                    .centered()
                    .render(main_area, buf);
            }
            _ => self.menu_view.render(main_area, buf),
        }

        self.log_widget.render(log_area, buf);

        let [autobackup_area, _, action_area] =
            Layout::horizontal([Constraint::Length(16), Constraint::Length(1), Constraint::Fill(1)]).areas(footer_area);

        let (autobackup_text, autobackup_style) = if self.engine_control.get_autobackup() {
            ("ON ", FOOTER_AUTOBACKUP_ON_STYLE)
        } else {
            ("OFF", FOOTER_AUTOBACKUP_OFF_STYLE)
        };

        Paragraph::new(format!("Autobackup {autobackup_text}"))
            .style(autobackup_style)
            .bold()
            .centered()
            .render(autobackup_area, buf);

        if let Some(action) = self.state.lock().unwrap().current_action.as_ref() {
            Gauge::default()
                .gauge_style(PROGRESS_BAR_STYLE)
                .bg(PROGRESS_BAR_BG_COLOR)
                .label(action.describe())
                .ratio(action.progress.get() as f64)
                .render(action_area, buf);
        } else {
            Line::raw("Idle").centered().render(action_area, buf);
        };
    }
}
