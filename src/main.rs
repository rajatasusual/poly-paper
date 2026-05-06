use std::{
    collections::{BTreeMap},
    time::{Duration, Instant},
};

use anyhow::{ Result};
use clap::Parser;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use polymarket_client_sdk_v2::{
    clob::ws::{BookUpdate, Client as wsClient},
    gamma::types::response::Market,
    types::{Decimal, U256},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::TableState};
use tokio::sync::mpsc;

mod types;
mod render;
mod helper;
mod handler;

use types::*;
use render::render;
use helper::*;
use handler::*;

// ── CLI ─────────────────────────────────────────

#[derive(Parser)]
struct Args {
    slug: Option<String>,
}

// subscribe_orderbook emits full snapshots, so replace the local side atomically.
fn replace_levels(book: &mut BTreeMap<Decimal, Decimal>, levels: Vec<(Decimal, Decimal)>) {
    book.clear();
    for (price, size) in levels {
        if size > Decimal::ZERO {
            book.insert(price, size);
        }
    }
}

// detect trades (very rough)
fn detect_trade(app: &mut AppState, old_bid: Option<Decimal>, old_ask: Option<Decimal>) {
    let new_bid = app.bids.keys().next_back().cloned();
    let new_ask = app.asks.keys().next().cloned();

    if let (Some(ob), Some(nb)) = (old_bid, new_bid) {
        if nb < ob {
            app.trades.push_front(Trade {
                price: nb,
                size: Decimal::from(1),
                side: "SELL",
            });
        }
    }

    if let (Some(oa), Some(na)) = (old_ask, new_ask) {
        if na > oa {
            app.trades.push_front(Trade {
                price: na,
                size: Decimal::from(1),
                side: "BUY",
            });
        }
    }

    if app.trades.len() > 20 {
        app.trades.pop_back();
    }
}

// ── WS TASK ─────────────────────────────────────

async fn ws_task(asset_ids: Vec<U256>, tx: mpsc::Sender<BookUpdate>) -> Result<()> {
    let client = wsClient::default();
    let mut stream = Box::pin(client.subscribe_orderbook(asset_ids)?);

    while let Some(result) = stream.next().await {
        if let Ok(book) = result {
            if tx.send(book).await.is_err() {
                break;
            }
        }
    }
    Ok(())
}

// ── MAIN ────────────────────────────────────────

async fn run_market_view(market: Market) -> Result<MarketViewExit> {
    let MarketSession { mut app, asset_ids } = market_session(market)?;
    let (tx, mut rx) = mpsc::channel(64);
    let ws_handle = tokio::spawn(async move {
        let _ = ws_task(asset_ids, tx).await;
    });

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut table_state = TableState::default();

    let loop_result = loop {
        while let Ok(update) = rx.try_recv() {
            if update.asset_id != app.asset_id {
                continue;
            }

            let start = Instant::now();

            let old_bid = app.bids.keys().next_back().cloned();
            let old_ask = app.asks.keys().next().cloned();

            let bids = update.bids.into_iter().map(|l| (l.price, l.size)).collect();
            let asks = update.asks.into_iter().map(|l| (l.price, l.size)).collect();

            replace_levels(&mut app.bids, bids);
            replace_levels(&mut app.asks, asks);

            detect_trade(&mut app, old_bid, old_ask);

            app.last_latency_ms = start.elapsed().as_millis();
            app.last_ts = update.timestamp.to_string();
        }

        terminal.draw(|f| render(f, &mut app, &mut table_state))?;

        if event::poll(Duration::from_millis(50))? {
            if let CEvent::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => break Ok(MarketViewExit::Query),
                    KeyCode::Esc => break Ok(MarketViewExit::Quit),
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
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

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    ws_handle.abort();

    loop_result
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut next_market = if let Some(slug) = args.slug.as_deref() {
        match resolve_market(slug).await {
            Ok(market) => Some(market),
            Err(err) => {
                println!("Could not resolve slug \"{slug}\": {err}");
                None
            }
        }
    } else {
        None
    };

    loop {
        let market = if let Some(market) = next_market.take() {
            market
        } else if let Some(market) = prompt_for_market().await? {
            market
        } else {
            break;
        };

        match run_market_view(market).await? {
            MarketViewExit::Query => {}
            MarketViewExit::Quit => break,
        }
    }

    Ok(())
}
