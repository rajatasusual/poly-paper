use std::{collections::BTreeMap, str::FromStr, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use polymarket_client_sdk_v2::{
    clob::ws::{BookUpdate, Client as wsClient},
    gamma::{types::{request::MarketBySlugRequest, response::Market}, Client as gammaClient},
    types::{Decimal, U256},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, Table},
    Terminal,
};
use tokio::sync::mpsc;

// ── CLI ─────────────────────────────────────────

#[derive(Parser)]
struct Args {
    slug: String,
}

// ── STATE ───────────────────────────────────────

struct AppState {
    slug: String,
    question: String,

    bids: BTreeMap<Decimal, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,

    tick_size: Decimal,

    last_ts: String,
}

// ── HELPERS ─────────────────────────────────────

async fn resolve_market(slug: &str) -> Result<Market> {
    let client = gammaClient::default();
    Ok(client
        .market_by_slug(&MarketBySlugRequest::builder().slug(slug).build())
        .await?)
}

fn bar(size: &Decimal) -> String {
    let n = (size.as_f64() * 10.0) as usize;
    "█".repeat(n.min(20))
}

fn aggregate(
    map: &BTreeMap<Decimal, Decimal>,
    tick: Decimal,
) -> BTreeMap<Decimal, Decimal> {
    let mut out = BTreeMap::new();
    for (p, s) in map {
        let k = (*p / tick).floor() * tick;
        *out.entry(k).or_insert(Decimal::ZERO) += *s;
    }
    out
}

// ── WS ──────────────────────────────────────────

async fn ws_task(asset_ids: Vec<String>, tx: mpsc::Sender<BookUpdate>) -> Result<()> {
    let ids: Vec<U256> = asset_ids
        .iter()
        .map(|s| U256::from_str(s).map_err(anyhow::Error::msg))
        .collect::<Result<_>>()?;

    let client = wsClient::default();
    let mut stream = Box::pin(client.subscribe_orderbook(ids)?);

    while let Some(result) = stream.next().await {
        match result {
            Ok(book) => {
                let update = BookUpdate::builder()
                    .asset_id(book.asset_id)
                    .timestamp(book.timestamp)
                    .bids(book.bids.iter().take(20).cloned().collect())
                    .asks(book.asks.iter().take(20).cloned().collect())
                    .market(book.market)
                    .build();

                if tx.send(update).await.is_err() {
                    break;
                }
            }
            Err(_) => {}
        }
    }
    Ok(())
}

// ── RENDER ──────────────────────────────────────

fn render(frame: &mut ratatui::Frame, app: &AppState) {
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    let best_bid = app.bids.keys().next_back().cloned();
    let best_ask = app.asks.keys().next().cloned();

    if best_bid.is_none() || best_ask.is_none() {
        return;
    }

    let best_bid = best_bid.unwrap();
    let best_ask = best_ask.unwrap();

    let bids = aggregate(&app.bids, app.tick_size);
    let asks = aggregate(&app.asks, app.tick_size);

    let mut prices: Vec<Decimal> = bids
        .keys()
        .chain(asks.keys())
        .cloned()
        .collect();

    prices.sort_by(|a, b| b.cmp(a));
    prices.dedup();

    let mut cum_bid = Decimal::ZERO;
    let mut cum_ask = Decimal::ZERO;

    let top_n = 5;
    let bid_sum: Decimal = bids.values().take(top_n).cloned().sum();
    let ask_sum: Decimal = asks.values().take(top_n).cloned().sum();

    let imbalance = if ask_sum.is_zero() {
        Decimal::ZERO
    } else {
        bid_sum / ask_sum
    };

    let rows: Vec<Row> = prices.iter().map(|price| {
        let bid = bids.get(price).cloned().unwrap_or_default();
        let ask = asks.get(price).cloned().unwrap_or_default();

        cum_bid += bid;
        cum_ask += ask;

        let is_best = *price == best_bid || *price == best_ask;

        let price_style = if is_best {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        Row::new(vec![
            Cell::from(format!("{:.2}", cum_bid)).style(Style::default().fg(Color::Green)),
            Cell::from(format!("{} {:.2}", bar(&bid), bid)).style(Style::default().fg(Color::Green)),
            Cell::from(format!("{:.4}", price)).style(price_style),
            Cell::from(format!("{} {:.2}", bar(&ask), ask)).style(Style::default().fg(Color::Red)),
            Cell::from(format!("{:.2}", cum_ask)).style(Style::default().fg(Color::Red)),
        ])
    }).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(16),
            Constraint::Length(10),
        ],
    )
    .header(Row::new(vec!["CumBid", "Bid", "Price", "Ask", "CumAsk"]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " {} | Imb {:.2} | Spread {:.4} ",
                app.slug,
                imbalance,
                best_ask - best_bid
            )),
    );

    frame.render_widget(table, layout[1]);

    frame.render_widget(
        ratatui::widgets::Paragraph::new(Span::styled(
            format!("{} | {}", app.question, app.last_ts),
            Style::default().fg(Color::DarkGray),
        )),
        layout[0],
    );

    frame.render_widget(
        ratatui::widgets::Paragraph::new("[q] quit | +/- aggregation"),
        layout[2],
    );
}

// ── MAIN ────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let market = resolve_market(&args.slug).await?;

    let question = market.question.context("no question")?;

    let tokens: Vec<String> = market
        .clob_token_ids
        .context("no tokens")?
        .iter()
        .map(|x| x.to_string())
        .collect();

    let mut app = AppState {
        slug: args.slug,
        question,
        bids: BTreeMap::new(),
        asks: BTreeMap::new(),
        tick_size: Decimal::from_str("0.01")?,
        last_ts: String::new(),
    };

    let (tx, mut rx) = mpsc::channel(64);

    tokio::spawn(ws_task(tokens, tx));

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    loop {
        while let Ok(update) = rx.try_recv() {
            app.last_ts = update.timestamp.to_string();

            app.bids.clear();
            app.asks.clear();

            for l in update.bids {
                if l.size > Decimal::ZERO {
                    app.bids.insert(l.price, l.size);
                }
            }

            for l in update.asks {
                if l.size > Decimal::ZERO {
                    app.asks.insert(l.price, l.size);
                }
            }
        }

        terminal.draw(|f| render(f, &app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => break,
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