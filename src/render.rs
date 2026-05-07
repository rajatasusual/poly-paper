// ── RENDER ──────────────────────────────────────

use std::collections::BTreeMap;

use polymarket_client_sdk_v2::types::Decimal;
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::types::*;

const VISIBLE_LEVELS: usize = 20;
const BAR_WIDTH: usize = 14;

enum BarAlign {
    Left,
    Right,
}

fn aggregate(map: &BTreeMap<Decimal, Decimal>, tick: Decimal) -> BTreeMap<Decimal, Decimal> {
    let mut out = BTreeMap::new();
    for (price, size) in map {
        let bucket = (*price / tick).floor() * tick;
        *out.entry(bucket).or_insert(Decimal::ZERO) += *size;
    }
    out
}

fn volume_bar(size: Decimal, max_size: Decimal, width: usize, align: BarAlign) -> String {
    if size <= Decimal::ZERO || max_size <= Decimal::ZERO || width == 0 {
        return " ".repeat(width);
    }

    let ratio = (size.as_f64() / max_size.as_f64()).clamp(0.0, 1.0);
    let eighths = ((ratio * width as f64 * 8.0).round() as usize).clamp(1, width * 8);
    let full_cells = eighths / 8;
    let partial = eighths % 8;
    let partials = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

    let filled_width = full_cells + usize::from(partial > 0);
    let empty_width = width.saturating_sub(filled_width);
    let full = "█".repeat(full_cells);

    match align {
        BarAlign::Left => {
            let mut bar = full;
            bar.push_str(partials[partial]);
            bar.push_str(&" ".repeat(empty_width));
            bar
        }
        BarAlign::Right => {
            let mut bar = " ".repeat(empty_width);
            bar.push_str(partials[partial]);
            bar.push_str(&full);
            bar
        }
    }
}

fn muted() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn header_style() -> Style {
    Style::default()
        .fg(Color::Gray)
        .add_modifier(Modifier::BOLD)
}

fn bid_style() -> Style {
    Style::default().fg(Color::Green)
}

fn ask_style() -> Style {
    Style::default().fg(Color::Red)
}

pub fn render(frame: &mut ratatui::Frame, app: &mut AppState, table_state: &mut TableState) {
    let layout = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(frame.area());

    let best_bid = app.bids.keys().next_back().cloned();
    let best_ask = app.asks.keys().next().cloned();

    if best_bid.is_none() || best_ask.is_none() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(&app.question, header_style())),
                Line::from(vec![
                    Span::styled("Outcome ", muted()),
                    Span::raw(&app.outcome),
                    Span::styled("   Status ", muted()),
                    Span::raw("waiting for book"),
                ]),
            ])
            .block(Block::default().borders(Borders::BOTTOM)),
            layout[0],
        );
        return;
    }

    let best_bid = best_bid.unwrap();
    let best_ask = best_ask.unwrap();

    let bids = aggregate(&app.bids, app.tick_size);
    let asks = aggregate(&app.asks, app.tick_size);

    let bid_levels: Vec<(Decimal, Decimal)> = bids
        .iter()
        .rev()
        .skip(app.scroll)
        .take(VISIBLE_LEVELS)
        .map(|(price, size)| (*price, *size))
        .collect();
    let ask_levels: Vec<(Decimal, Decimal)> = asks
        .iter()
        .skip(app.scroll)
        .take(VISIBLE_LEVELS)
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
            let (bid_cum, bid_size, bid_bar, bid_price) =
                if let Some((price, size)) = bid_levels.get(i) {
                    cum_bid += *size;
                    (
                        format!("{:.2}", cum_bid),
                        format!("{:.2}", size),
                        volume_bar(*size, max_size, BAR_WIDTH, BarAlign::Right),
                        format!("{:.4}", price),
                    )
                } else {
                    (
                        String::new(),
                        String::new(),
                        " ".repeat(BAR_WIDTH),
                        String::new(),
                    )
                };

            let (ask_price, ask_bar, ask_size, ask_cum) =
                if let Some((price, size)) = ask_levels.get(i) {
                    cum_ask += *size;
                    (
                        format!("{:.4}", price),
                        volume_bar(*size, max_size, BAR_WIDTH, BarAlign::Left),
                        format!("{:.2}", size),
                        format!("{:.2}", cum_ask),
                    )
                } else {
                    (
                        String::new(),
                        " ".repeat(BAR_WIDTH),
                        String::new(),
                        String::new(),
                    )
                };

            let price_style = if i == 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(bid_cum).style(muted()),
                Cell::from(bid_size).style(bid_style()),
                Cell::from(bid_bar).style(bid_style()),
                Cell::from(bid_price).style(price_style),
                Cell::from(ask_price).style(price_style),
                Cell::from(ask_bar).style(ask_style()),
                Cell::from(ask_size).style(ask_style()),
                Cell::from(ask_cum).style(muted()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(BAR_WIDTH as u16),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(BAR_WIDTH as u16),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec![
            "Bid Cum", "Bid Size", "Bid Vol", "Bid", "Ask", "Ask Vol", "Ask Size", "Ask Cum",
        ])
        .style(header_style()),
    )
    .block(Block::default().borders(Borders::ALL).title(format!(
        " Order Book | {} | {} | Spread {:.4} ",
        app.slug,
        app.outcome,
        best_ask - best_bid
    )))
    .column_spacing(1);

    frame.render_stateful_widget(table, layout[1], table_state);

    let tape: Vec<Span> = app
        .trades
        .iter()
        .map(|t| {
            let color = match t.side {
                TradeSide::BuyCompleteSet => Color::Green,
                TradeSide::SellCompleteSet => Color::Red,
            };
            Span::styled(
                format!(
                    "{} {:.2}@{:.4} +{:.4} ",
                    t.side.label(),
                    t.size,
                    t.price,
                    t.profit
                ),
                Style::default().fg(color),
            )
        })
        .collect();

    let tape = if tape.is_empty() {
        Line::from(Span::styled("No arbitrage executions yet", muted()))
    } else {
        Line::from(tape)
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Arbitrage ", header_style()),
                Span::styled("(complete-set)", muted()),
            ]),
            tape,
        ])
        .block(Block::default().borders(Borders::TOP)),
        layout[2],
    );

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(&app.question, header_style())),
            Line::from(vec![
                Span::styled("Outcome ", muted()),
                Span::raw(&app.outcome),
                Span::styled("   Best Bid ", muted()),
                Span::styled(format!("{:.4}", best_bid), bid_style()),
                Span::styled("   Best Ask ", muted()),
                Span::styled(format!("{:.4}", best_ask), ask_style()),
                Span::styled("   Tick ", muted()),
                Span::raw(app.tick_size.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Latency ", muted()),
                Span::raw(format!("{}ms", app.last_latency_ms)),
                Span::styled("   Updated ", muted()),
                Span::raw(&app.last_ts),
                Span::styled("   Commands ", muted()),
                Span::raw("q search  Esc quit  +/- tick  ↑/↓ scroll"),
            ]),
        ])
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::BOTTOM)),
        layout[0],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_bar_always_matches_column_width() {
        let bar = volume_bar(Decimal::from(1), Decimal::from(3), 14, BarAlign::Left);

        assert_eq!(bar.chars().count(), 14);
    }

    #[test]
    fn volume_bar_fills_column_at_max_size() {
        let bar = volume_bar(Decimal::from(5), Decimal::from(5), 8, BarAlign::Left);

        assert_eq!(bar, "████████");
    }

    #[test]
    fn right_aligned_volume_bar_grows_toward_price() {
        let bar = volume_bar(Decimal::from(1), Decimal::from(2), 8, BarAlign::Right);

        assert_eq!(bar, "    ████");
    }
}
