//! In-process state for a single `eoka-server` session: at most one
//! `Browser`, plus the pages opened on it, keyed by `Page::target_id()`.

use std::collections::HashMap;

use eoka::{Browser, Page};

use crate::protocol::ServerError;

#[derive(Default)]
pub struct AppState {
    browser: Option<Browser>,
    pages: HashMap<String, Page>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_browser(&mut self, browser: Browser) {
        self.browser = Some(browser);
    }

    pub fn take_browser(&mut self) -> Option<Browser> {
        self.browser.take()
    }

    pub fn is_launched(&self) -> bool {
        self.browser.is_some()
    }

    /// Every method other than `browser.launch` goes through this to reach
    /// the browser. PROTOCOL.md's error table has no dedicated "not
    /// launched" code, so an unlaunched browser is deliberately folded into
    /// `Internal`.
    pub fn browser(&self) -> Result<&Browser, ServerError> {
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
}
