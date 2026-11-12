//! Which page or folder is on screen, and how to move between them.
//!
//! Main pages are numbered 1 to `page_count`. A folder is a single extra page,
//! opened from a main page; `back` returns to that main page.

/// Where a button lives, and what the deck is currently showing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum View {
    Page(u8),
    Folder(String),
}

pub struct Navigator {
    page_count: u8,
    /// Current main page, or the one the open folder was opened from.
    page: u8,
    folder: Option<String>,
}

impl Navigator {
    pub fn new(page_count: u8) -> Self {
        Self {
            page_count,
            page: 1,
            folder: None,
        }
    }

    pub fn view(&self) -> View {
        match &self.folder {
            Some(name) => View::Folder(name.clone()),
            None => View::Page(self.page),
        }
    }

    /// Text for the `page_number` display: "2/3", or the folder name.
    pub fn label(&self) -> String {
        match &self.folder {
            Some(name) => name.clone(),
            None => format!("{}/{}", self.page, self.page_count),
        }
    }

    /// Next main page, wrapping to 1. Also leaves an open folder.
    pub fn next_page(&mut self) {
        self.folder = None;
        self.page = self.page % self.page_count + 1;
    }

    /// Previous main page, wrapping to the last. Also leaves an open folder.
    pub fn previous_page(&mut self) {
        self.folder = None;
        self.page = if self.page == 1 {
            self.page_count
        } else {
            self.page - 1
        };
    }

    /// Ignored if the page does not exist (config validation prevents it anyway).
    pub fn go_to_page(&mut self, page: u8) {
        if (1..=self.page_count).contains(&page) {
            self.folder = None;
            self.page = page;
        }
    }

    pub fn open_folder(&mut self, name: &str) {
        self.folder = Some(name.to_string());
    }

    pub fn back(&mut self) {
        self.folder = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_wrap_around() {
        let mut nav = Navigator::new(3);
        nav.previous_page();
        assert_eq!(nav.view(), View::Page(3));
        nav.next_page();
        assert_eq!(nav.view(), View::Page(1));
        assert_eq!(nav.label(), "1/3");
    }

    #[test]
    fn folder_back_returns_to_opening_page() {
        let mut nav = Navigator::new(3);
        nav.go_to_page(2);
        nav.open_folder("music");
        assert_eq!(nav.view(), View::Folder("music".into()));
        assert_eq!(nav.label(), "music");
        nav.back();
        assert_eq!(nav.view(), View::Page(2));
    }

    #[test]
    fn go_to_missing_page_is_ignored() {
        let mut nav = Navigator::new(2);
        nav.go_to_page(5);
        assert_eq!(nav.view(), View::Page(1));
    }
}
