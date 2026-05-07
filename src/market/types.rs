// ── TRADE EVENT ─────────────────────────────────

use std::{
    collections::{BTreeMap, VecDeque},
    time::{SystemTime, UNIX_EPOCH},
};

use polymarket_client_sdk_v2::{
    gamma::types::response::Market,
    types::{Decimal, U256},
};
use serde::Serialize;

#[derive(Clone)]
pub enum TradeSide {
    BuyCompleteSet,
    SellCompleteSet,
}

impl TradeSide {
    pub fn label(&self) -> &'static str {
        match self {
            Self::BuyCompleteSet => "BUY SET",
            Self::SellCompleteSet => "SELL SET",
        }
    }
}

#[derive(Clone)]
pub struct Trade {
    pub price: Decimal,
    pub size: Decimal,
    pub side: TradeSide,
    pub profit: Decimal,
}

#[derive(Serialize)]
pub struct PaperOutcome {
    pub label: String,
    pub asset_id: String,
}

#[derive(Clone)]
pub struct OutcomeToken {
    pub label: String,
    pub asset_id: U256,
}

#[derive(Clone, Default)]
pub struct OrderBook {
    pub bids: BTreeMap<Decimal, Decimal>,
    pub asks: BTreeMap<Decimal, Decimal>,
}

impl OrderBook {
    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids
            .iter()
            .next_back()
            .map(|(price, size)| (*price, *size))
    }

    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(price, size)| (*price, *size))
    }
}

#[derive(Clone, Copy)]
pub enum ArbitrageKind {
    BuyCompleteSet,
    SellCompleteSet,
}

impl ArbitrageKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::BuyCompleteSet => "buy_complete_set",
            Self::SellCompleteSet => "sell_complete_set",
        }
    }

    pub fn trade_side(&self) -> TradeSide {
        match self {
            Self::BuyCompleteSet => TradeSide::BuyCompleteSet,
            Self::SellCompleteSet => TradeSide::SellCompleteSet,
        }
    }
}

#[derive(Clone)]
pub struct ArbitrageLeg {
    pub outcome: String,
    pub asset_id: U256,
    pub side: String,
    pub price: String,
    pub size: String,
}

#[derive(Clone)]
pub struct ArbitrageOpportunity {
    pub kind: ArbitrageKind,
    pub package_price: Decimal,
    pub max_size: Decimal,
    pub profit_per_set: Decimal,
    pub signature: String,
    pub legs: Vec<ArbitrageLeg>,
}

#[derive(Serialize)]
pub struct PaperExecutionLeg {
    pub outcome: String,
    pub asset_id: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub available_size: String,
    pub notional: String,
}

#[derive(Serialize)]
pub struct PaperExecution {
    pub strategy: String,
    pub package_price: String,
    pub size: String,
    pub profit_per_set: String,
    pub guaranteed_profit: String,
    pub collateral: String,
    pub cash_after: String,
    pub pending_settlement_payout_after: String,
    pub source: String,
    pub market_timestamp: String,
    pub recorded_unix_ms: u128,
    pub legs: Vec<PaperExecutionLeg>,
}

#[derive(Serialize)]
pub struct PaperTrade {
    pub slug: String,
    pub question: String,
    pub outcomes: Vec<PaperOutcome>,
    pub started_unix_ms: u128,
    pub ended_unix_ms: Option<u128>,
    pub exit_reason: Option<String>,
    pub starting_cash: String,
    pub cash: String,
    pub pending_settlement_payout: String,
    pub realized_pnl: String,
    pub locked_pnl: String,
    pub total_pnl: String,
    pub executions: Vec<PaperExecution>,
    #[serde(skip)]
    executed_sizes_by_signature: BTreeMap<String, String>,
}

impl PaperTrade {
    pub fn new(
        slug: &str,
        question: &str,
        outcomes: &[OutcomeToken],
        starting_cash: Decimal,
    ) -> Self {
        Self {
            slug: slug.to_owned(),
            question: question.to_owned(),
            outcomes: outcomes
                .iter()
                .map(|outcome| PaperOutcome {
                    label: outcome.label.clone(),
                    asset_id: outcome.asset_id.to_string(),
                })
                .collect(),
            started_unix_ms: unix_ms(),
            ended_unix_ms: None,
            exit_reason: None,
            starting_cash: starting_cash.to_string(),
            cash: starting_cash.to_string(),
            realized_pnl: Decimal::ZERO.to_string(),
            locked_pnl: Decimal::ZERO.to_string(),
            pending_settlement_payout: Decimal::ZERO.to_string(),
            total_pnl: Decimal::ZERO.to_string(),
            executions: Vec::new(),
            executed_sizes_by_signature: BTreeMap::new(),
        }
    }

    pub fn finish(&mut self, reason: &str) {
        if reason == "market_closed" {
            self.settle_complete_sets();
        }
        self.ended_unix_ms = Some(unix_ms());
        self.exit_reason = Some(reason.to_owned());
        self.refresh_total_pnl();
    }

    pub fn execute_opportunity(
        &mut self,
        opportunity: ArbitrageOpportunity,
        market_timestamp: &str,
    ) -> Option<Trade> {
        let already_executed = self.executed_size(&opportunity.signature);
        let remaining_size = opportunity.max_size - already_executed;
        if remaining_size <= Decimal::ZERO || opportunity.profit_per_set <= Decimal::ZERO {
            return None;
        }

        let cash = decimal_from_string(&self.cash);
        let affordable_size = match opportunity.kind {
            ArbitrageKind::BuyCompleteSet => {
                if opportunity.package_price <= Decimal::ZERO {
                    Decimal::ZERO
                } else {
                    cash / opportunity.package_price
                }
            }
            ArbitrageKind::SellCompleteSet => cash,
        };
        let size = remaining_size.min(affordable_size);
        if size <= Decimal::ZERO {
            return None;
        }

        let guaranteed_profit = opportunity.profit_per_set * size;
        let collateral = match opportunity.kind {
            ArbitrageKind::BuyCompleteSet => self.buy_complete_set(&opportunity, size),
            ArbitrageKind::SellCompleteSet => self.sell_complete_set(&opportunity, size),
        };

        self.executed_sizes_by_signature.insert(
            opportunity.signature.clone(),
            (already_executed + size).to_string(),
        );
        self.record_execution(
            &opportunity,
            size,
            guaranteed_profit,
            collateral,
            market_timestamp,
        );
        self.refresh_total_pnl();

        Some(Trade {
            price: opportunity.package_price,
            size,
            side: opportunity.kind.trade_side(),
            profit: guaranteed_profit,
        })
    }

    fn buy_complete_set(&mut self, opportunity: &ArbitrageOpportunity, size: Decimal) -> Decimal {
        let cash = decimal_from_string(&self.cash);
        let pending_settlement_payout = decimal_from_string(&self.pending_settlement_payout);
        let locked_pnl = decimal_from_string(&self.locked_pnl);
        let cost = opportunity.package_price * size;
        let guaranteed_profit = opportunity.profit_per_set * size;

        self.cash = (cash - cost).to_string();
        self.pending_settlement_payout = (pending_settlement_payout + size).to_string();
        self.locked_pnl = (locked_pnl + guaranteed_profit).to_string();
        Decimal::ZERO
    }

    fn sell_complete_set(&mut self, opportunity: &ArbitrageOpportunity, size: Decimal) -> Decimal {
        let cash = decimal_from_string(&self.cash);
        let realized_pnl = decimal_from_string(&self.realized_pnl);
        let guaranteed_profit = opportunity.profit_per_set * size;

        self.cash = (cash + guaranteed_profit).to_string();
        self.realized_pnl = (realized_pnl + guaranteed_profit).to_string();
        size
    }

    fn settle_complete_sets(&mut self) {
        let pending_settlement_payout = decimal_from_string(&self.pending_settlement_payout);
        if pending_settlement_payout <= Decimal::ZERO {
            return;
        }

        let cash = decimal_from_string(&self.cash);
        let realized_pnl = decimal_from_string(&self.realized_pnl);
        let locked_pnl = decimal_from_string(&self.locked_pnl);

        self.cash = (cash + pending_settlement_payout).to_string();
        self.pending_settlement_payout = Decimal::ZERO.to_string();
        self.realized_pnl = (realized_pnl + locked_pnl).to_string();
        self.locked_pnl = Decimal::ZERO.to_string();
    }

    fn refresh_total_pnl(&mut self) {
        let realized_pnl = decimal_from_string(&self.realized_pnl);
        let locked_pnl = decimal_from_string(&self.locked_pnl);
        self.total_pnl = (realized_pnl + locked_pnl).to_string();
    }

    fn record_execution(
        &mut self,
        opportunity: &ArbitrageOpportunity,
        size: Decimal,
        guaranteed_profit: Decimal,
        collateral: Decimal,
        market_timestamp: &str,
    ) {
        let legs = opportunity
            .legs
            .iter()
            .map(|leg| PaperExecutionLeg {
                outcome: leg.outcome.clone(),
                asset_id: leg.asset_id.to_string(),
                side: leg.side.clone(),
                price: leg.price.clone(),
                size: size.to_string(),
                available_size: leg.size.clone(),
                notional: (decimal_from_string(&leg.price) * size).to_string(),
            })
            .collect();

        self.executions.push(PaperExecution {
            strategy: opportunity.kind.label().to_owned(),
            package_price: opportunity.package_price.to_string(),
            size: size.to_string(),
            profit_per_set: opportunity.profit_per_set.to_string(),
            guaranteed_profit: guaranteed_profit.to_string(),
            collateral: collateral.to_string(),
            cash_after: self.cash.clone(),
            pending_settlement_payout_after: self.pending_settlement_payout.clone(),
            source: "detect_trade".to_owned(),
            market_timestamp: market_timestamp.to_owned(),
            recorded_unix_ms: unix_ms(),
            legs,
        });
    }

    fn executed_size(&self, signature: &str) -> Decimal {
        self.executed_sizes_by_signature
            .get(signature)
            .map(|size| decimal_from_string(size))
            .unwrap_or(Decimal::ZERO)
    }
}

// ── STATE ───────────────────────────────────────

pub struct AppState {
    pub slug: String,
    pub question: String,
    pub outcome: String,
    pub asset_id: U256,
    pub outcomes: Vec<OutcomeToken>,

    pub bids: BTreeMap<Decimal, Decimal>,
    pub asks: BTreeMap<Decimal, Decimal>,
    pub books: BTreeMap<U256, OrderBook>,

    pub last_latency_ms: u128,

    pub trades: VecDeque<Trade>,

    pub paper_trade: PaperTrade,

    pub tick_size: Decimal,
    pub scroll: usize,

    pub last_ts: String,
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn decimal_from_string(value: &str) -> Decimal {
    value.parse().unwrap_or(Decimal::ZERO)
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
    MarketClosed,
}

pub enum MarketPickerResult {
    Selected(Box<Market>),
    Back,
    Query,
    Quit,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn paper_trade_executes_buy_complete_set_arbitrage() {
        let outcomes = vec![
            OutcomeToken {
                label: "Yes".to_owned(),
                asset_id: U256::from(1),
            },
            OutcomeToken {
                label: "No".to_owned(),
                asset_id: U256::from(2),
            },
        ];
        let mut paper = PaperTrade::new("market-slug", "Question?", &outcomes, Decimal::from(100));

        let trade = paper.execute_opportunity(
            ArbitrageOpportunity {
                kind: ArbitrageKind::BuyCompleteSet,
                package_price: Decimal::from_str("0.98").unwrap(),
                max_size: Decimal::from(10),
                profit_per_set: Decimal::from_str("0.02").unwrap(),
                signature: "buy:0.40|0.58".to_owned(),
                legs: vec![
                    ArbitrageLeg {
                        outcome: "Yes".to_owned(),
                        asset_id: U256::from(1),
                        side: "BUY".to_owned(),
                        price: "0.40".to_owned(),
                        size: "10".to_owned(),
                    },
                    ArbitrageLeg {
                        outcome: "No".to_owned(),
                        asset_id: U256::from(2),
                        side: "BUY".to_owned(),
                        price: "0.58".to_owned(),
                        size: "20".to_owned(),
                    },
                ],
            },
            "1",
        );

        assert!(trade.is_some());
        assert_eq!(paper.executions.len(), 1);
        assert_eq!(paper.cash, "90.20");
        assert_eq!(paper.pending_settlement_payout, "10");
        assert_eq!(paper.locked_pnl, "0.20");
        assert_eq!(paper.total_pnl, "0.20");

        paper.finish("market_closed");

        assert_eq!(paper.cash, "100.20");
        assert_eq!(paper.pending_settlement_payout, "0");
        assert_eq!(paper.realized_pnl, "0.20");
        assert_eq!(paper.locked_pnl, "0");
    }

    #[test]
    fn paper_trade_executes_sell_complete_set_arbitrage_once_per_price_level() {
        let outcomes = vec![
            OutcomeToken {
                label: "Yes".to_owned(),
                asset_id: U256::from(1),
            },
            OutcomeToken {
                label: "No".to_owned(),
                asset_id: U256::from(2),
            },
        ];
        let mut paper = PaperTrade::new("market-slug", "Question?", &outcomes, Decimal::from(100));
        let opportunity = ArbitrageOpportunity {
            kind: ArbitrageKind::SellCompleteSet,
            package_price: Decimal::from_str("1.03").unwrap(),
            max_size: Decimal::from(5),
            profit_per_set: Decimal::from_str("0.03").unwrap(),
            signature: "sell:0.52|0.51".to_owned(),
            legs: vec![
                ArbitrageLeg {
                    outcome: "Yes".to_owned(),
                    asset_id: U256::from(1),
                    side: "SELL".to_owned(),
                    price: "0.52".to_owned(),
                    size: "7".to_owned(),
                },
                ArbitrageLeg {
                    outcome: "No".to_owned(),
                    asset_id: U256::from(2),
                    side: "SELL".to_owned(),
                    price: "0.51".to_owned(),
                    size: "5".to_owned(),
                },
            ],
        };

        assert!(
            paper
                .execute_opportunity(opportunity.clone(), "1")
                .is_some()
        );
        assert!(paper.execute_opportunity(opportunity, "2").is_none());

        assert_eq!(paper.executions.len(), 1);
        assert_eq!(paper.cash, "100.15");
        assert_eq!(paper.realized_pnl, "0.15");
        assert_eq!(paper.total_pnl, "0.15");
    }
}
