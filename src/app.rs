use std::{io, time::Duration};

use anyhow::Result;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use polymarket_client_sdk_v2::{gamma::types::response::Market, types::Decimal};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::TableState};
use tokio::sync::mpsc;

use crate::{
    render::render,
    session::market_session,
    types::{MarketSession, MarketViewExit},
    ws::ws_task,
};

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub async fn run_market_view(market: Market) -> Result<MarketViewExit> {
    let MarketSession { mut app, asset_ids } = market_session(market)?;
    let (tx, mut rx) = mpsc::channel(64);
    let ws_handle = tokio::spawn(async move {
        let _ = ws_task(asset_ids, tx).await;
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

        terminal.draw(|frame| render(frame, &mut app, &mut table_state))?;

        if event::poll(Duration::from_millis(50))? {
            if let CEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break Ok(MarketViewExit::Query),
                    KeyCode::Esc => break Ok(MarketViewExit::Quit),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break Ok(MarketViewExit::Quit);
                    }
                    KeyCode::Down => app.scroll += 1,
                    KeyCode::Up => app.scroll = app.scroll.saturating_sub(1),
                    KeyCode::Char('+') => app.tick_size *= Decimal::from(2),
                    KeyCode::Char('-') => app.tick_size /= Decimal::from(2),
                    _ => {}
                }
            }
        }
    };

    ws_handle.abort();
    loop_result
}
