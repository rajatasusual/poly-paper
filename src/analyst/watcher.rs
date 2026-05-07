use notify::{RecursiveMode, Watcher};
use std::sync::mpsc::channel;
use std::thread;

pub fn watch_logs(path: &str, reload_tx: std::sync::mpsc::Sender<()>) -> notify::Result<()> {
    let path = path.to_string();
    let (tx, rx) = channel();

    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

    thread::spawn(move || {
        let _watcher = watcher;
        while let Ok(event) = rx.recv() {
            if event.is_ok() {
                let _ = reload_tx.send(());
            }
        }
    });

    Ok(())
}
