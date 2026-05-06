// ── TRADE EVENT ─────────────────────────────────

use std::collections::{BTreeMap, VecDeque};

use polymarket_client_sdk_v2::{gamma::types::response::Market, types::{Decimal, U256}};

#[derive(Clone)]
pub struct Trade {
    pub price: Decimal,
    pub size: Decimal,
    pub side: &'static str, // "BUY" or "SELL"
}

// ── STATE ───────────────────────────────────────

pub struct AppState {
    pub slug: String,
    pub question: String,
    pub outcome: String,
    pub asset_id: U256,

    pub bids: BTreeMap<Decimal, Decimal>,
    pub asks: BTreeMap<Decimal, Decimal>,

    pub last_latency_ms: u128,

    pub trades: VecDeque<Trade>,

    pub tick_size: Decimal,
    pub scroll: usize,

    pub last_ts: String,
}

pub const SEARCH_PAGE_SIZE: i32 = 5;
pub const MARKET_PAGE_SIZE: usize = 8;

pub struct EventChoice {
    pub label: String,
    pub markets: Vec<Market>,
}

pub struct EventSearchPage {
    pub choices: Vec<EventChoice>,
    pub page: i32,
    pub has_more: bool,
}

pub struct MarketSession {
    pub app: AppState,
    pub asset_ids: Vec<U256>,
}

pub enum MarketViewExit {
    Query,
    Quit,
}

pub enum MarketPickerResult {
    Selected(Market),
    Back,
    Query,
    Quit,
}
