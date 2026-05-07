use crate::models::Session;

pub struct AppState {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub filter: String,
    pub should_quit: bool,
}

impl AppState {
    pub fn next(&mut self) {
        if self.selected + 1 < self.sessions.len() {
            self.selected += 1;
        }
    }

    pub fn previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
}