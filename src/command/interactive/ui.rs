use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::internal::sync::SyncUiHandler;

const OVERALL_TEMPLATE: &str = " {spinner:.blue} {wide_msg:.blue}";
const SCAN_TEMPLATE: &str = " {spinner:.blue} {wide_msg:.blue}";
const PREPARE_TEMPLATE: &str = " {spinner:.blue} {wide_msg:.blue}";

const SYNC_TEMPLATE: &str = " {prefix:>8} [{bar:40.cyan/blue}] {pos}/{len}, ETA: {eta} {wide_msg:.blue}";
const FILE_TEMPLATE: &str = " {prefix:>8} [{bar:40.cyan/blue}] {bytes}/{total_bytes} {wide_msg:.blue}";

const PROGRESS_CHARS: &str = "●●·";

pub struct FancyUiHandler {
    multi_progress: MultiProgress,

    backup_name: Option<String>,
    sync_message: String,

    overall_pb: Option<ProgressBar>,
    scan_pb: Option<ProgressBar>,
    prepare_pb: Option<ProgressBar>,
    sync_pb: Option<ProgressBar>,
    staging_pb: Option<ProgressBar>,
    compress_pb: Option<ProgressBar>,
    file_pb: Option<ProgressBar>,
}

impl FancyUiHandler {
    pub fn new() -> Self {
        Self {
            multi_progress: MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(5)),

            backup_name: None,
            sync_message: String::new(),

            overall_pb: None,
            scan_pb: None,
            prepare_pb: None,
            sync_pb: None,
            staging_pb: None,
            compress_pb: None,
            file_pb: None,
        }
    }

    pub fn clear(self) -> Result<(), anyhow::Error> {
        self.multi_progress.clear()?;

        Ok(())
    }

    pub fn begin_backup(&mut self, name: &str) {
        self.backup_name = Some(name.to_owned());

        //self.multi_progress.println(format!("Creating backup: {name}")).ok();

        let pb = ProgressBar::new_spinner()
            .with_style(ProgressStyle::default_bar().template(OVERALL_TEMPLATE).unwrap())
            .with_message(format!("Creating backup: {name}"));

        let pb = self.multi_progress.add(pb);

        pb.enable_steady_tick(Duration::from_millis(120));

        self.overall_pb = Some(pb);
    }

    pub fn end_backup(&mut self, success: bool) {
        let name = self.backup_name.take().unwrap_or_default();

        let Some(pb) = self.overall_pb.take() else {
            return;
        };

        let message = if success {
            format!("Backup successfully created: {name}")
        } else {
            format!("Backup failed: {name}")
        };

        pb.finish_with_message(message);
    }

    pub fn begin_staging(&mut self, count: usize) {
        let pb = ProgressBar::new(count as u64)
            .with_style(
                ProgressStyle::default_bar()
                    .template(SYNC_TEMPLATE)
                    .unwrap()
                    .progress_chars(PROGRESS_CHARS),
            )
            .with_prefix("Staging")
            .with_message("Staging save paths...");

        let pb = self.multi_progress.add(pb);

        self.staging_pb = Some(pb);
    }

    pub fn begin_stage(&mut self, name: &str) {
        self.sync_message = name.to_owned();

        let Some(pb) = self.staging_pb.as_ref() else {
            return;
        };

        pb.set_message(name.to_owned());
    }

    pub fn end_stage(&mut self) {
        let Some(pb) = self.staging_pb.as_ref() else {
            return;
        };

        pb.inc(1);
    }

    pub fn end_staging(&mut self) {
        self.sync_message.clear();

        let Some(pb) = self.staging_pb.take() else {
            return;
        };

        pb.finish_and_clear();
    }

    pub fn begin_compress(&mut self) {
        let pb = ProgressBar::new_spinner()
            .with_style(ProgressStyle::default_bar().template(PREPARE_TEMPLATE).unwrap())
            .with_message("Compressing...");

        let pb = self.multi_progress.add(pb);

        pb.enable_steady_tick(Duration::from_millis(120));

        self.compress_pb = Some(pb);
    }

    pub fn end_compress(&mut self) {
        let Some(pb) = self.compress_pb.take() else {
            return;
        };

        pb.finish_and_clear();
    }

    pub fn begin_restore(&mut self, name: &str) {
        self.backup_name = Some(name.to_owned());

        //self.multi_progress.println(format!("Restoring backup: {name}")).ok();

        let pb = ProgressBar::new_spinner()
            .with_style(ProgressStyle::default_bar().template(OVERALL_TEMPLATE).unwrap())
            .with_message(format!("Restoring backup: {name}"));

        let pb = self.multi_progress.add(pb);

        pb.enable_steady_tick(Duration::from_millis(120));

        self.overall_pb = Some(pb);
    }

    pub fn end_restore(&mut self, success: bool) {
        self.sync_message.clear();

        let name = self.backup_name.take().unwrap_or_default();

        let Some(pb) = self.overall_pb.take() else {
            return;
        };

        let message = if success {
            format!("Backup successfully restored: {name}")
        } else {
            format!("Restore backup failed: {name}")
        };

        pb.finish_with_message(message);
    }

    pub fn begin_extract(&mut self) {
        let pb = ProgressBar::new_spinner()
            .with_style(ProgressStyle::default_bar().template(PREPARE_TEMPLATE).unwrap())
            .with_message("Extracting...");

        let pb = self.multi_progress.add(pb);

        pb.enable_steady_tick(Duration::from_millis(120));

        self.compress_pb = Some(pb);
    }

    pub fn end_extract(&mut self) {
        let Some(pb) = self.compress_pb.take() else {
            return;
        };

        pb.finish_and_clear();
    }

    pub fn begin_restore_sp(&mut self, name: &str) {
        self.sync_message = name.to_owned();

        let Some(pb) = self.staging_pb.as_ref() else {
            return;
        };

        pb.set_message(name.to_owned());
    }

    pub fn end_restore_sp(&mut self) {
        let Some(pb) = self.staging_pb.as_ref() else {
            return;
        };

        pb.inc(1);
    }
}

impl SyncUiHandler for FancyUiHandler {
    fn begin_scan(&mut self) {
        let pb = ProgressBar::new_spinner()
            .with_style(ProgressStyle::default_bar().template(SCAN_TEMPLATE).unwrap())
            .with_message("Scanning directory content...");

        let pb = self.multi_progress.add(pb);

        pb.enable_steady_tick(Duration::from_millis(120));

        self.scan_pb = Some(pb);
    }

    fn end_scan(&mut self) {
        if let Some(pb) = self.scan_pb.take() {
            pb.finish_and_clear();
        }
    }

    fn begin_prepare(&mut self) {
        let pb = ProgressBar::new_spinner()
            .with_style(ProgressStyle::default_bar().template(PREPARE_TEMPLATE).unwrap())
            .with_message("Preparing...");

        let pb = self.multi_progress.add(pb);

        pb.enable_steady_tick(Duration::from_millis(120));

        self.prepare_pb = Some(pb);
    }

    fn end_prepare(&mut self) {
        let Some(pb) = self.prepare_pb.take() else {
            return;
        };

        pb.finish_and_clear();
    }

    fn begin_sync(&mut self, op_count: usize) {
        let pb = ProgressBar::new(op_count as u64)
            .with_style(
                ProgressStyle::default_bar()
                    .template(SYNC_TEMPLATE)
                    .unwrap()
                    .progress_chars(PROGRESS_CHARS),
            )
            .with_prefix("Syncing")
            .with_message(self.sync_message.to_owned());

        let pb = self.multi_progress.add(pb);

        self.sync_pb = Some(pb);
    }

    fn sync_progress(&mut self) {
        let Some(pb) = self.sync_pb.as_ref() else {
            return;
        };

        pb.inc(1);
    }

    fn end_sync(&mut self) {
        let Some(pb) = self.sync_pb.take() else {
            return;
        };

        pb.finish_and_clear();
    }

    fn begin_file(&mut self, prefix: &str, filename: &str, size: u64) {
        let pb = ProgressBar::new(size)
            .with_style(
                ProgressStyle::default_bar()
                    .template(FILE_TEMPLATE)
                    .unwrap()
                    .progress_chars(PROGRESS_CHARS),
            )
            .with_prefix(prefix.to_owned())
            .with_message(filename.to_owned());

        let pb = self.multi_progress.add(pb);

        self.file_pb = Some(pb);
    }

    fn file_progress(&mut self, bytes: u64) {
        let Some(pb) = self.file_pb.as_ref() else {
            return;
        };

        pb.inc(bytes);
    }

    fn end_file(&mut self) {
        let Some(pb) = self.file_pb.take() else {
            return;
        };

        pb.finish_and_clear();
    }
}
