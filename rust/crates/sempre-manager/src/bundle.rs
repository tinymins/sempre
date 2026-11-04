use sempre_bundle::Export;

use crate::{Manager, ManagerError, VersionRunner};

impl<R: VersionRunner> Manager<R> {
    pub fn export_bundle(&self) -> Result<Export, ManagerError> {
        // Installing on a router copies a package and never reads a bundle.
        self.reject_on_openwrt("Bundle export")?;
        let document = self.store.read()?;
        let executable = std::env::current_exe()
            .map_err(|source| ManagerError::io("locate current executable", source))?;
        Ok(sempre_bundle::export(
            self.store.layout(),
            &document,
            &executable,
        )?)
    }
}
