use std::path::PathBuf;

/// Manages shared browser context and data directories.
/// Compatible with wry::WebContext.
pub struct WebContext {
    data_directory: Option<PathBuf>,
    allows_automation: bool,
}

impl WebContext {
    pub fn new(data_directory: Option<PathBuf>) -> Self {
        Self {
            data_directory,
            allows_automation: false,
        }
    }

    pub fn set_allows_automation(&mut self, flag: bool) {
        self.allows_automation = flag;
    }

    pub fn data_directory(&self) -> Option<&PathBuf> {
        self.data_directory.as_ref()
    }

    pub fn allows_automation(&self) -> bool {
        self.allows_automation
    }
}
