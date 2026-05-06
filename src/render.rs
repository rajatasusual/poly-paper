// ── RENDER ──────────────────────────────────────

use std::collections::BTreeMap;

use polymarket_client_sdk_v2::types::Decimal;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::types::*;

fn aggregate(map: &BTreeMap<Decimal, Decimal>, tick: Decimal) -> BTreeMap<Decimal, Decimal> {
    let mut out = BTreeMap::new();
    for (price, size) in map {
        let bucket = (*price / tick).floor() * tick;
        *out.entry(bucket).or_insert(Decimal::ZERO) += *size;
    }
    out
}

fn bar(size: &Decimal, max_size: Decimal) -> String {
    if *size <= Decimal::ZERO || max_size <= Decimal::ZERO {
        return String::new();
    }

    let ratio = (size.as_f64() / max_size.as_f64()).clamp(0.0, 1.0);
    let n = (ratio * 20.0).round() as usize;
    "█".repeat(n.max(1))
}

pub fn render(frame: &mut ratatui::Frame, app: &mut AppState, table_state: &mut TableState) {
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
