use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use polymarket_client_sdk_v2::gamma::types::{request::MarketBySlugRequest, response::Market};
use polymarket_client_sdk_v2::{
    clob::{types::response::ClobToken, ws::Client as wsClient},
    gamma::Client as gammaClient,
    types::U256,
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};
use tokio::sync::mpsc;

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(about = "Polymarket orderbook TUI")]
struct Args {
    /// Market slug, e.g. "will-trump-win-2024"
    slug: String,
}

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Default)]
struct OrderSide {
    entries: Vec<(String, String)>, // (price, size)
}

struct AppState {
    slug: String,
    question: String,
    condition_id: String,
    tokens: Vec<String>,
    outcomes: Vec<String>,
    bids: Vec<OrderSide>,
    asks: Vec<OrderSide>,
    last_ts: String,
    status: String,
}

impl AppState {
fn new(condition_id: String, slug: String, question: String, tokens: Vec<ClobToken>) -> Self {
        let n = tokens.len();
        let outcomes = tokens.iter().map(|t| t.outcome.clone()).collect();
        let tokens = tokens.iter().map(|t| t.token_id.to_string()).collect();
        Self {
            slug,
            question,
            condition_id,
            tokens,
            outcomes,
            bids: (0..n).map(|_| OrderSide::default()).collect(),
            asks: (0..n).map(|_| OrderSide::default()).collect(),
            last_ts: String::new(),
            status: "Connecting...".into(),
        }
    }
}

// ── WS message ───────────────────────────────────────────────────────────────

struct BookUpdate {
    asset_id: String,
    timestamp: String,
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
}

// ── Gamma + CLOB REST helpers ─────────────────────────────────────────────────

/// Resolves a slug to (condition_id_string, question, asset_ids, outcomes) via the Gamma SDK client.
async fn resolve_market(slug: &str) -> Result<Market> {
    let client = gammaClient::default();
    let market = client
        .market_by_slug(&MarketBySlugRequest::builder().slug(slug).build())
        .await?;

    Ok(market)
}


// ── WebSocket task ────────────────────────────────────────────────────────────

async fn ws_task(asset_ids: Vec<String>, tx: mpsc::Sender<BookUpdate>) -> Result<()> {
    let ids: Vec<U256> = asset_ids
        .iter()
        .map(|s| U256::from_str(s).map_err(anyhow::Error::msg))
        .collect::<Result<_>>()?;

    let client = wsClient::default();
    let stream = client.subscribe_orderbook(ids)?;
    let mut stream = Box::pin(stream);

    while let Some(result) = stream.next().await {
        match result {
            Ok(book) => {
                let update = BookUpdate {
                    asset_id: book.asset_id.to_string(),
                    timestamp: book.timestamp.to_string(),
                    bids: book
                        .bids
                        .iter()
                        .take(10)
                        .map(|b| (b.price.to_string(), b.size.to_string()))
                        .collect(),
                    asks: book
                        .asks
                        .iter()
                        .take(10)
                        .map(|a| (a.price.to_string(), a.size.to_string()))
                        .collect(),
                };
                if tx.send(update).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                let _ = tx
                    .send(BookUpdate {
                        asset_id: String::new(),
                        timestamp: format!("error: {e}"),
                        bids: vec![],
                        asks: vec![],
                    })
                    .await;
            }
        }
    }
    Ok(())
}

// ── UI rendering ──────────────────────────────────────────────────────────────

fn render_orderbook(
    frame: &mut ratatui::Frame,
    state: &AppState,
    table_states: &mut Vec<TableState>,
) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    // Header
    let header_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", state.slug))
        .title(format!(" ({}) ", state.condition_id));
    let header_inner = header_block.inner(outer[0]);
    frame.render_widget(header_block, outer[0]);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(format!(
            "{} | last update: {}",
            state.question, state.last_ts
        )),
        header_inner,
    );

    // One column per outcome
    let n = state.outcomes.len().max(1);
    let cols: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n as u32)).collect();
    let outcome_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(cols)
        .split(outer[1]);

    for (i, area) in outcome_areas.iter().enumerate() {
        let label = state.outcomes.get(i).cloned().unwrap_or_default();
        let sides = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(*area);

        // Asks (top)
        let ask_rows: Vec<Row> = state.asks[i]
            .entries
            .iter()
            .map(|(p, s)| {
                Row::new(vec![
                    Cell::from(p.clone()).style(Style::default().fg(Color::Red)),
                    Cell::from(s.clone()),
                ])
            })
            .collect();

        let ask_table = Table::new(
            ask_rows,
            [Constraint::Percentage(50), Constraint::Percentage(50)],
        )
        .header(
            Row::new(vec!["Price", "Size"]).style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {label} — Asks ")),
        );
        frame.render_stateful_widget(ask_table, sides[0], &mut table_states[i * 2]);

        // Bids (bottom)
        let bid_rows: Vec<Row> = state.bids[i]
            .entries
            .iter()
            .map(|(p, s)| {
                Row::new(vec![
                    Cell::from(p.clone()).style(Style::default().fg(Color::Green)),
                    Cell::from(s.clone()),
                ])
            })
            .collect();

        let bid_table = Table::new(
            bid_rows,
            [Constraint::Percentage(50), Constraint::Percentage(50)],
        )
        .header(
            Row::new(vec!["Price", "Size"]).style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {label} — Bids ")),
        );
        frame.render_stateful_widget(bid_table, sides[1], &mut table_states[i * 2 + 1]);
    }

    // Status bar
    frame.render_widget(
        ratatui::widgets::Paragraph::new(Span::styled(
            format!(" {}  [q] quit", state.status),
            Style::default().fg(Color::DarkGray),
        )),
        outer[2],
    );
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    eprintln!("Resolving market '{}'…", args.slug);
    let market= resolve_market(&args.slug).await?;

    let condition_id = market
        .condition_id
        .context("market has no condition_id")?
        .to_string();
    let question = market
        .question
        .context("market has no question")?
        .to_string();

    let tokens: Vec<String> = market.clob_token_ids
        .context("market has no clob_token_ids")?
        .iter()
        .map(|id| id.to_string())
        .collect();

    let outcomes = market.outcomes.context("market has no outcomes")?.clone();

    eprintln!("Found {} tokens, connecting to WS…", tokens.len());

    // Build AppState directly from (id, outcome) pairs
    let mut app = AppState {
        condition_id,
        slug: args.slug.clone(),
        question,
        tokens: tokens.clone(),
        outcomes,
        bids: (0..tokens.len()).map(|_| OrderSide::default()).collect(),
        asks: (0..tokens.len()).map(|_| OrderSide::default()).collect(),
        last_ts: String::new(),
        status: "Connecting...".into(),
    };

    // WS → UI channel
    let (tx, mut rx) = mpsc::channel::<BookUpdate>(64);
    let tokens_clone = app.tokens.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_task(tokens_clone, tx).await {
            eprintln!("WS error: {e}");
        }
    });

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let n = app.outcomes.len();
    let mut table_states: Vec<TableState> = (0..n * 2).map(|_| TableState::default()).collect();

    app.status = "Connected — waiting for data…".into();

    // Main loop
    loop {
        // Drain all pending WS messages
        while let Ok(update) = rx.try_recv() {
            if update.asset_id.is_empty() {
                app.status = update.timestamp.clone(); // error message
            } else {
                app.last_ts = update.timestamp.clone();
                app.status = format!("Live — {}", update.asset_id);
                if let Some(idx) = app.tokens.iter().position(|id| *id == update.asset_id) {
                    app.bids[idx].entries = update.bids;
                    app.asks[idx].entries = update.asks;
                }
            }
        }

        terminal.draw(|f| render_orderbook(f, &app, &mut table_states))?;

        // Input with short poll to stay responsive
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break;
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
