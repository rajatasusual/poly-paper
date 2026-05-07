use std::{
    collections::{BTreeMap, VecDeque},
    str::FromStr,
};

use anyhow::{Context, Result};
use polymarket_client_sdk_v2::{gamma::types::response::Market, types::Decimal};

use crate::market::types::{AppState, MarketSession, OrderBook, OutcomeToken, PaperTrade};

pub fn market_session(market: Market) -> Result<MarketSession> {
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

    let asset_ids = market.clob_token_ids.context("no tokens")?;
    let asset_id = *asset_ids.first().context("no token ids")?;
    let outcome_labels = market.outcomes.unwrap_or_default();
    let outcomes: Vec<OutcomeToken> = asset_ids
        .iter()
        .enumerate()
        .map(|(index, asset_id)| OutcomeToken {
            label: outcome_labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("Outcome {}", index + 1)),
            asset_id: *asset_id,
        })
        .collect();
    let books = outcomes
        .iter()
        .map(|outcome| (outcome.asset_id, OrderBook::default()))
        .collect();
    let starting_cash = Decimal::from(100);
    let paper_trade = PaperTrade::new(&slug, &question, &outcomes, starting_cash);

    Ok(MarketSession {
        app: AppState {
            slug,
            question,
            outcome,
            asset_id,
            outcomes,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            books,
            last_latency_ms: 0,
            trades: VecDeque::new(),
            paper_trade,
            tick_size: Decimal::from_str("0.01").unwrap(),
            scroll: 0,
            last_ts: String::new(),
        },
        asset_ids,
    })
}
