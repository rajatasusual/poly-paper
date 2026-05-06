use std::{
    collections::{BTreeMap, VecDeque},
    str::FromStr,
};

use anyhow::{Context, Result};
use polymarket_client_sdk_v2::{gamma::types::response::Market, types::Decimal};

use crate::types::{AppState, MarketSession};

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
