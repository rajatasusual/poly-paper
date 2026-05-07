use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Result;
use chrono::Utc;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use polymarket_client_sdk_v2::{gamma::types::response::Market, types::Decimal};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::TableState};
use tokio::sync::mpsc;

use crate::{
    market::gamma::resolve_market,
    market::render::render,
    market::session::market_session,
    market::types::{MarketSession, MarketViewExit, PaperTrade},
    market::ws::ws_task,
};

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub async fn run_market_view(market: Market) -> Result<MarketViewExit> {
    let market_end_date = market.end_date.clone();
    let MarketSession { mut app, asset_ids } = market_session(market)?;
    let (tx, mut rx) = mpsc::channel(64);
    let ws_handle = tokio::spawn(async move {
        let _ = ws_task(asset_ids, tx).await;
    });

    let (closed_tx, mut closed_rx) = mpsc::channel(1);
    let closed_slug = app.slug.clone();
    let closed_handle = tokio::spawn(async move {
        let mut fast_tick = tokio::time::interval(Duration::from_millis(10));
        let mut resolve_tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = fast_tick.tick() => {
                    if let Some(end_date) = market_end_date.as_ref() {
                        if Utc::now() >= *end_date {
                            let _ = closed_tx.send(()).await;
                            break;
                        }
                    }
                }
                _ = resolve_tick.tick() => {
                    match resolve_market(&closed_slug).await {
                        Ok(market) if market.closed.unwrap_or(false) => {
                            let _ = closed_tx.send(()).await;
                            break;
                        }
                        Ok(_) | Err(_) => {}
                    }
                }
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen) {
        disable_raw_mode()?;
        return Err(err.into());
    }
    let _terminal_guard = TerminalGuard;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let mut table_state = TableState::default();

    let loop_result = loop {
        while let Ok(update) = rx.try_recv() {
            app.apply_book_update(update);
        }
        if closed_rx.try_recv().is_ok() {
            app.paper_trade.finish("market_closed");
            break Ok(MarketViewExit::MarketClosed);
        }

        terminal.draw(|frame| render(frame, &mut app, &mut table_state))?;

        if event::poll(Duration::from_millis(50))?
            && let CEvent::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') => {
                    app.paper_trade.finish("user_query");
                    break Ok(MarketViewExit::Query);
                }
                KeyCode::Esc => {
                    app.paper_trade.finish("user_quit");
                    break Ok(MarketViewExit::Quit);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.paper_trade.finish("user_quit");
                    break Ok(MarketViewExit::Quit);
                }
                KeyCode::Down => app.scroll += 1,
                KeyCode::Up => app.scroll = app.scroll.saturating_sub(1),
                KeyCode::Char('+') => app.tick_size *= Decimal::from(2),
                KeyCode::Char('-') => app.tick_size /= Decimal::from(2),
                _ => {}
            }
        }
    };

    ws_handle.abort();
    closed_handle.abort();
    drop(terminal);
    drop(_terminal_guard);
    let log_path = save_paper_trade_log(&app.paper_trade)?;
    println!("Saved paper trade log: {}", log_path.display());
    loop_result
}

fn save_paper_trade_log(paper_trade: &PaperTrade) -> Result<PathBuf> {
    let logs_dir = Path::new("logs");
    fs::create_dir_all(logs_dir)?;

    let file_name = format!("{}.json", safe_file_stem(&paper_trade.slug));
    let path = logs_dir.join(file_name);
    fs::write(&path, serde_json::to_string_pretty(paper_trade)?)?;
    Ok(path)
}

fn safe_file_stem(value: &str) -> String {
    let stem = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "selected-market".to_owned()
    } else {
        stem.to_owned()
    }
}
