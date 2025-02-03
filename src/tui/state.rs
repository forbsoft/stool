use std::time::Instant;

#[derive(Debug)]
pub enum ActionKind {
    CreateBackup { name: String },
    RestoreBackup { name: String },
}

#[derive(Clone, Debug, Default)]
pub enum Progress {
    Exact(f32),
    Estimate {
        start: Instant,
        end: Instant,
    },

    #[default]
    Unknown,
}

#[derive(Debug)]
pub struct Action {
    pub kind: ActionKind,
    pub started_at: Instant,
    pub progress: Progress,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub current_action: Option<Action>,
}

impl Action {
    pub fn new(kind: ActionKind) -> Self {
        Self {
            kind,
            started_at: Instant::now(),
            progress: Progress::default(),
        }
    }

    pub fn describe(&self) -> String {
        let description = self.kind.describe();

        match self.progress {
            Progress::Unknown => description,
            _ => {
                let percent = self.progress.get() * 100.;

                format!("{description}... {percent:>3.0}%")
            }
        }
    }
}

impl ActionKind {
    pub fn describe(&self) -> String {
        match self {
            Self::CreateBackup { name } => format!("Creating backup: {name}"),
            Self::RestoreBackup { name } => format!("Restoring backup: {name}"),
        }
    }

    pub fn describe_complete(&self) -> String {
        match self {
            Self::CreateBackup { name } => format!("Backup created: {name}"),
            Self::RestoreBackup { name } => format!("Backup restored: {name}"),
        }
    }

    pub fn describe_error(&self) -> String {
        match self {
            Self::CreateBackup { name } => format!("Create backup failed: {name}"),
            Self::RestoreBackup { name } => format!("Restore backup failed: {name}"),
        }
    }
}

impl Progress {
    pub fn set(&mut self, value: f32) {
        *self = Self::Exact(value);
    }

    pub fn get(&self) -> f32 {
        match self {
            Self::Exact(v) => *v,
            Self::Estimate { start, end } => {
                let now = Instant::now();
                let total = *end - *start;
                let elapsed = now - *start;

                (elapsed.as_secs_f32() / total.as_secs_f32()).clamp(0., 0.99)
            }
            Self::Unknown => 0.,
        }
    }
}
