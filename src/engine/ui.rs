use crate::internal::sync::SyncUiHandler;

pub trait StoolUiHandler: SyncUiHandler + 'static + Send {
    fn clear(self) -> Result<(), anyhow::Error>;

    fn begin_backup(&mut self, name: &str);
    fn end_backup(&mut self, success: bool);

    fn begin_staging(&mut self, count: usize);
    fn begin_stage(&mut self, name: &str);
    fn end_stage(&mut self);
    fn end_staging(&mut self);

    fn begin_compress(&mut self);
    fn end_compress(&mut self);

    fn begin_restore(&mut self, name: &str);
    fn end_restore(&mut self, success: bool);

    fn begin_extract(&mut self);
    fn end_extract(&mut self);

    fn begin_restore_sp(&mut self, name: &str);
    fn end_restore_sp(&mut self);
}
