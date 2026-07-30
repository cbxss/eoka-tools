use std::collections::HashMap;
use std::sync::Arc;

use eoka::{Browser, Page};

use crate::protocol::ServerError;

#[derive(Default)]
pub struct AppState {
    browser: Option<Arc<Browser>>,
    pages: HashMap<String, Page>,
    shutdown: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_browser(&mut self, browser: Browser) {
        self.browser = Some(Arc::new(browser));
    }

    pub fn take_browser(&mut self) -> Option<Arc<Browser>> {
        self.browser.take()
    }

    pub fn is_launched(&self) -> bool {
        self.browser.is_some()
    }

    pub fn browser(&self) -> Result<&Arc<Browser>, ServerError> {
        self.browser
            .as_ref()
            .ok_or_else(|| ServerError::internal("browser not launched; call browser.launch first"))
    }

    pub fn insert_page(&mut self, id: String, page: Page) {
        self.pages.insert(id, page);
    }

    pub fn remove_page(&mut self, id: &str) -> Option<Page> {
        self.pages.remove(id)
    }

    pub fn page(&self, id: &str) -> Result<&Page, ServerError> {
        self.pages
            .get(id)
            .ok_or_else(|| ServerError::invalid_page(id))
    }

    pub fn clear_pages(&mut self) {
        self.pages.clear();
    }

    pub fn mark_shutdown(&mut self) {
        self.shutdown = true;
    }

    pub fn should_shutdown(&self) -> bool {
        self.shutdown
    }
}
