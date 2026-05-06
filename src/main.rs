use std::{
    collections::{BTreeMap, VecDeque},
    str::FromStr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use polymarket_client_sdk_v2::{
    clob::ws::{BookUpdate, Client as wsClient},
    gamma::{
        Client as gammaClient,
        types::{request::MarketBySlugRequest, response::Market},
    },
    types::{Decimal, U256},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
use tokio::sync::mpsc;

// ── CLI ─────────────────────────────────────────

#[derive(Parser)]
struct Args {
    slug: String,
}

// ── TRADE EVENT ─────────────────────────────────

#[derive(Clone)]
struct Trade {
    price: Decimal,
    size: Decimal,
    side: &'static str, // "BUY" or "SELL"
}

// ── STATE ───────────────────────────────────────

struct AppState {
    slug: String,
    question: String,
    outcome: String,
    asset_id: U256,

    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,

    last_latency_ms: u128,

    trades: VecDeque<Trade>,

    tick_size: Decimal,
    scroll: usize,

    last_ts: String,
}

// ── HELPERS ─────────────────────────────────────

async fn resolve_market(slug: &str) -> Result<Market> {
    let client = gammaClient::default();
    Ok(client
        .market_by_slug(&MarketBySlugRequest::builder().slug(slug).build())
        .await?)
}

fn bar(size: &Decimal, max_size: Decimal) -> String {
    if *size <= Decimal::ZERO || max_size <= Decimal::ZERO {
        return String::new();
    }

    let ratio = (size.as_f64() / max_size.as_f64()).clamp(0.0, 1.0);
    let n = (ratio * 20.0).round() as usize;
    "█".repeat(n.max(1))
}

fn aggregate(map: &BTreeMap<Decimal, Decimal>, tick: Decimal) -> BTreeMap<Decimal, Decimal> {
    let mut out = BTreeMap::new();
    for (price, size) in map {
        let bucket = (*price / tick).floor() * tick;
        *out.entry(bucket).or_insert(Decimal::ZERO) += *size;
    }
    out
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

// ── RENDER ──────────────────────────────────────

fn render(frame: &mut ratatui::Frame, app: &mut AppState, table_state: &mut TableState) {
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(5),
    ])
    .split(frame.area());

    let best_bid = app.bids.keys().next_back().cloned();
    let best_ask = app.asks.keys().next().cloned();

    if best_bid.is_none() || best_ask.is_none() {
        frame.render_widget(
            Paragraph::new(format!("{} | waiting for {}", app.question, app.outcome)),
            layout[0],
        );
        return;
    }

    let best_bid = best_bid.unwrap();
    let best_ask = best_ask.unwrap();

    let bids = aggregate(&app.bids, app.tick_size);
    let asks = aggregate(&app.asks, app.tick_size);

    let visible = 20;
    let bid_levels: Vec<(Decimal, Decimal)> = bids
        .iter()
        .rev()
        .skip(app.scroll)
        .take(visible)
        .map(|(price, size)| (*price, *size))
        .collect();
    let ask_levels: Vec<(Decimal, Decimal)> = asks
        .iter()
        .skip(app.scroll)
        .take(visible)
        .map(|(price, size)| (*price, *size))
        .collect();
    let max_size = bid_levels
        .iter()
        .chain(ask_levels.iter())
        .map(|(_, size)| *size)
        .max()
        .unwrap_or(Decimal::ZERO);

    let mut cum_bid = Decimal::ZERO;
    let mut cum_ask = Decimal::ZERO;

    let rows: Vec<Row> = (0..bid_levels.len().max(ask_levels.len()))
        .map(|i| {
            let (bid_cum, bid_size, bid_price) = if let Some((price, size)) = bid_levels.get(i) {
                cum_bid += *size;
                (
                    format!("{:.2}", cum_bid),
                    format!("{} {:.2}", bar(size, max_size), size),
                    format!("{:.4}", price),
                )
            } else {
                (String::new(), String::new(), String::new())
            };

            let (ask_price, ask_size, ask_cum) = if let Some((price, size)) = ask_levels.get(i) {
                cum_ask += *size;
                (
                    format!("{:.4}", price),
                    format!("{:.2} {}", size, bar(size, max_size)),
                    format!("{:.2}", cum_ask),
                )
            } else {
                (String::new(), String::new(), String::new())
            };

            let bid_price_style = if i == 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let ask_price_style = if i == 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(bid_cum).style(Color::Green),
                Cell::from(bid_size).style(Color::Green),
                Cell::from(bid_price).style(bid_price_style),
                Cell::from(ask_price).style(ask_price_style),
                Cell::from(ask_size).style(Color::Red),
                Cell::from(ask_cum).style(Color::Red),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(16),
            Constraint::Length(10),
        ],
    )
    .header(Row::new(vec![
        "CumBid", "Bid", "BidPx", "AskPx", "Ask", "CumAsk",
    ]))
    .block(Block::default().borders(Borders::ALL).title(format!(
        "{} | {} | Spread {:.4} | Tick {} | Lat {}ms",
        app.slug,
        app.outcome,
        best_ask - best_bid,
        app.tick_size,
        app.last_latency_ms
    )));

    frame.render_stateful_widget(table, layout[1], table_state);

    // trade tape
    let tape: Vec<Span> = app
        .trades
        .iter()
        .map(|t| {
            let color = if t.side == "BUY" {
                Color::Green
            } else {
                Color::Red
            };
            Span::styled(
                format!("{} {:.2}@{:.4} ", t.side, t.size, t.price),
                Style::default().fg(color),
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(Line::from(tape)), layout[2]);

    frame.render_widget(
        Paragraph::new(format!("{} | {}", app.question, app.last_ts)),
        layout[0],
    );
}

// ── MAIN ────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let market = resolve_market(&args.slug).await?;
    let question = market.question.as_ref().context("no question")?.to_owned();
    let outcome = market
        .outcomes
        .as_ref()
        .and_then(|outcomes| outcomes.first())
        .cloned()
        .unwrap_or_else(|| "selected token".to_string());

    let asset_ids: Vec<U256> = market
        .clob_token_ids
        .context("no tokens")?
        .into_iter()
        .collect();
    let asset_id = *asset_ids.first().context("no token ids")?;

    let mut app = AppState {
        slug: args.slug,
        question,
        outcome,
        asset_id,
        bids: BTreeMap::new(),
        asks: BTreeMap::new(),
        last_latency_ms: 0,
        trades: VecDeque::new(),
        tick_size: Decimal::from_str("0.01")?,
        scroll: 0,
        last_ts: String::new(),
    };

    let (tx, mut rx) = mpsc::channel(64);
    tokio::spawn(ws_task(asset_ids, tx));

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut table_state = TableState::default();

    loop {
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
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down => app.scroll += 1,
                    KeyCode::Up => app.scroll = app.scroll.saturating_sub(1),
                    KeyCode::Char('+') => app.tick_size *= Decimal::from(2),
                    KeyCode::Char('-') => app.tick_size /= Decimal::from(2),
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
