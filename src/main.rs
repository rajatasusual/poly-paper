use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Write},
    str::FromStr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use polymarket_client_sdk_v2::{
    clob::ws::{BookUpdate, Client as wsClient},
    gamma::{
        Client as gammaClient,
        types::{
            request::{MarketBySlugRequest, SearchRequest},
            response::{Event as GammaEvent, Market},
        },
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
    slug: Option<String>,
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

const SEARCH_PAGE_SIZE: i32 = 5;
const MARKET_PAGE_SIZE: usize = 8;

struct EventChoice {
    label: String,
    markets: Vec<Market>,
}

struct EventSearchPage {
    choices: Vec<EventChoice>,
    page: i32,
    has_more: bool,
}

struct MarketSession {
    app: AppState,
    asset_ids: Vec<U256>,
}

enum MarketViewExit {
    Query,
    Quit,
}

// ── HELPERS ─────────────────────────────────────

async fn resolve_market(slug: &str) -> Result<Market> {
    let client = gammaClient::default();
    Ok(client
        .market_by_slug(&MarketBySlugRequest::builder().slug(slug).build())
        .await?)
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

fn has_clob_tokens(market: &Market) -> bool {
    market
        .clob_token_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty())
}

fn event_title(event: &GammaEvent) -> String {
    event
        .title
        .as_deref()
        .or(event.slug.as_deref())
        .unwrap_or("untitled event")
        .to_owned()
}

fn market_label(market: &Market) -> String {
    let question = market
        .question
        .as_deref()
        .or(market.slug.as_deref())
        .unwrap_or("untitled market");
    let slug = market.slug.as_deref().unwrap_or("no-slug");

    format!("{question} [{slug}]")
}

async fn search_event_page(query: &str, page: i32) -> Result<EventSearchPage> {
    let client = gammaClient::default();
    let results = client
        .search(
            &SearchRequest::builder()
                .q(query)
                .limit_per_type(SEARCH_PAGE_SIZE)
                .page(page)
                .keep_closed_markets(0)
                .search_tags(false)
                .search_profiles(false)
                .build(),
        )
        .await?;

    let mut choices = Vec::new();
    for event in results.events.unwrap_or_default() {
        let markets: Vec<Market> = event
            .markets
            .as_ref()
            .into_iter()
            .flatten()
            .filter(|market| !market.closed.unwrap_or(false) && has_clob_tokens(market))
            .cloned()
            .collect();

        if markets.is_empty() {
            continue;
        }

        choices.push(EventChoice {
            label: event_title(&event),
            markets,
        });
    }

    let has_more = results
        .pagination
        .and_then(|pagination| pagination.has_more)
        .unwrap_or(false);

    Ok(EventSearchPage {
        choices,
        page,
        has_more,
    })
}

fn print_event_page(query: &str, page: &EventSearchPage) {
    println!();
    println!("Events for \"{query}\" - page {}", page.page);
    for (i, choice) in page.choices.iter().enumerate() {
        println!(
            "{:>2}. {} ({} markets)",
            i + 1,
            choice.label,
            choice.markets.len()
        );
    }
    println!();
}

fn print_market_page(event: &EventChoice, page: usize) {
    let start = page * MARKET_PAGE_SIZE;
    let end = (start + MARKET_PAGE_SIZE).min(event.markets.len());

    println!();
    println!(
        "Markets for \"{}\" - page {}",
        event.label,
        page.saturating_add(1)
    );
    for (i, market) in event.markets[start..end].iter().enumerate() {
        println!("{:>2}. {}", i + 1, market_label(market));
    }
    println!();
}

enum MarketPickerResult {
    Selected(Market),
    Back,
    Query,
    Quit,
}

fn prompt_for_event_market(event: &EventChoice) -> Result<MarketPickerResult> {
    let mut page = 0;

    loop {
        print_market_page(event, page);

        let selection = prompt_line("Select market, n/p page, b events, q query, x quit: ")?;
        match selection.as_str() {
            "" | "b" => return Ok(MarketPickerResult::Back),
            "q" => return Ok(MarketPickerResult::Query),
            "x" => return Ok(MarketPickerResult::Quit),
            "n" => {
                if (page + 1) * MARKET_PAGE_SIZE < event.markets.len() {
                    page += 1;
                } else {
                    println!("Already on the last market page.");
                }
            }
            "p" => {
                if page > 0 {
                    page -= 1;
                } else {
                    println!("Already on the first market page.");
                }
            }
            _ => {
                if let Ok(index) = selection.parse::<usize>() {
                    if index > 0 {
                        let market_index = page * MARKET_PAGE_SIZE + index - 1;
                        if let Some(market) = event.markets.get(market_index) {
                            return Ok(MarketPickerResult::Selected(market.clone()));
                        }
                    }
                }

                println!("Unrecognized market selection.");
            }
        }
    }
}

async fn prompt_for_market() -> Result<Option<Market>> {
    loop {
        let mut query = prompt_line("Search Polymarket (blank to quit): ")?;
        if query.is_empty() {
            return Ok(None);
        }

        let mut page = 1;

        loop {
            let event_page = search_event_page(&query, page).await?;
            if event_page.choices.is_empty() {
                if event_page.has_more {
                    println!(
                        "No active CLOB events on page {page} for \"{query}\". Try n for the next page or enter a new query."
                    );
                } else {
                    println!("No active CLOB events found for \"{query}\". Try another query.");
                    break;
                }
            } else {
                print_event_page(&query, &event_page);
            }

            let selection = prompt_line("Select event, n/p page, q query, x quit, or new query: ")?;
            match selection.as_str() {
                "" | "q" => break,
                "x" => return Ok(None),
                "n" => {
                    if event_page.has_more {
                        page += 1;
                    } else {
                        println!("Already on the last event page.");
                    }
                }
                "p" => {
                    if page > 1 {
                        page -= 1;
                    } else {
                        println!("Already on the first event page.");
                    }
                }
                _ => {
                    if let Ok(index) = selection.parse::<usize>() {
                        if index > 0 {
                            if let Some(event) = event_page.choices.get(index - 1) {
                                match prompt_for_event_market(event)? {
                                    MarketPickerResult::Selected(market) => {
                                        return Ok(Some(market));
                                    }
                                    MarketPickerResult::Back => {}
                                    MarketPickerResult::Query => break,
                                    MarketPickerResult::Quit => return Ok(None),
                                }
                                continue;
                            }
                        }
                    }

                    query = selection;
                    page = 1;
                }
            }
        }
    }
}

fn market_session(market: Market) -> Result<MarketSession> {
    let slug = market
        .slug
        .clone()
        .unwrap_or_else(|| "selected-market".to_string());
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

    Ok(MarketSession {
        app: AppState {
            slug,
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
        },
        asset_ids,
    })
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

            let price_style = if i == 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(bid_cum).style(Color::Green),
                Cell::from(bid_size).style(Color::Green),
                Cell::from(bid_price).style(price_style),
                Cell::from(ask_price).style(price_style),
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
            Constraint::Length(10),
            Constraint::Length(16),
            Constraint::Length(12),
        ],
    )
    .header(Row::new(vec![
        "CumBid", "Bid", "BidPx", "AskPx", "Ask", "CumAsk",
    ]))
    .block(Block::default().borders(Borders::ALL).title(format!(
        "{} | {} | Spread {:.4} | Tick {} | Lat {}ms | q: search | Esc: quit",
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
