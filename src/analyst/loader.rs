use crate::analyst::models::Session;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn load_sessions(path: &str) -> Result<Vec<Session>> {
    let path = Path::new(path);
    fs::create_dir_all(path)?;

    let mut sessions = vec![];

    for entry in fs::read_dir(path)? {
        let entry = entry?;

        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(entry.path())?;

        if let Ok(session) = serde_json::from_str::<Session>(&content) {
            sessions.push(session);
        }
    }

    sessions.sort_by_key(|s| s.started_unix_ms);

    Ok(sessions)
}
