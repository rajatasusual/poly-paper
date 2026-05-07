use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub slug: String,
    pub question: String,
    pub started_unix_ms: i64,
    pub ended_unix_ms: i64,
    pub realized_pnl: String,
    pub total_pnl: String,
    pub cash: String,
    pub executions: Vec<Execution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub strategy: String,
    pub package_price: String,
    pub size: String,
    pub guaranteed_profit: String,
    pub collateral: String,
    pub cash_after: String,
    pub pending_settlement_payout_after: String,
    pub market_timestamp: String,
    pub legs: Vec<Leg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leg {
    pub outcome: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub notional: String,
}
