use std::{collections::BTreeMap, time::Instant};

use polymarket_client_sdk_v2::{clob::ws::BookUpdate, types::Decimal};

use crate::market::types::{AppState, ArbitrageKind, ArbitrageLeg, ArbitrageOpportunity};

const COMPLETE_SET_PAYOUT: u8 = 1;

// subscribe_orderbook emits full snapshots, so replace the local side atomically.
fn replace_levels(
    book: &mut BTreeMap<Decimal, Decimal>,
    levels: impl IntoIterator<Item = (Decimal, Decimal)>,
) {
    book.clear();
    for (price, size) in levels {
        if size > Decimal::ZERO {
            book.insert(price, size);
        }
    }
}

impl AppState {
    pub fn apply_book_update(&mut self, update: BookUpdate) {
        if !self.books.contains_key(&update.asset_id) {
            return;
        }

        let start = Instant::now();
        let is_selected_book = update.asset_id == self.asset_id;
        let book = self.books.entry(update.asset_id).or_default();

        replace_levels(
            &mut book.bids,
            update
                .bids
                .into_iter()
                .map(|level| (level.price, level.size)),
        );
        replace_levels(
            &mut book.asks,
            update
                .asks
                .into_iter()
                .map(|level| (level.price, level.size)),
        );

        if is_selected_book {
            self.bids = book.bids.clone();
            self.asks = book.asks.clone();
        }

        self.last_latency_ms = start.elapsed().as_millis();
        self.last_ts = update.timestamp.to_string();
        self.detect_trade();
    }

    // Detect complete-set arbitrage across all outcomes, not single-book price movement.
    fn detect_trade(&mut self) {
        for opportunity in self.arbitrage_opportunities() {
            if let Some(trade) = self
                .paper_trade
                .execute_opportunity(opportunity, &self.last_ts)
            {
                self.trades.push_front(trade);
            }
        }

        while self.trades.len() > 20 {
            self.trades.pop_back();
        }
    }

    pub fn arbitrage_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();
        if let Some(opportunity) = self.buy_complete_set_opportunity() {
            opportunities.push(opportunity);
        }
        if let Some(opportunity) = self.sell_complete_set_opportunity() {
            opportunities.push(opportunity);
        }
        opportunities
    }

    fn buy_complete_set_opportunity(&self) -> Option<ArbitrageOpportunity> {
        let mut package_price = Decimal::ZERO;
        let mut max_size: Option<Decimal> = None;
        let mut legs = Vec::new();

        for outcome in &self.outcomes {
            let book = self.books.get(&outcome.asset_id)?;
            let (price, size) = book.best_ask()?;
            package_price += price;
            max_size = Some(max_size.map_or(size, |current| current.min(size)));
            legs.push(ArbitrageLeg {
                outcome: outcome.label.clone(),
                asset_id: outcome.asset_id,
                side: "BUY".to_owned(),
                price: price.to_string(),
                size: size.to_string(),
            });
        }

        let payout = Decimal::from(COMPLETE_SET_PAYOUT);
        if package_price >= payout {
            return None;
        }

        Some(ArbitrageOpportunity {
            kind: ArbitrageKind::BuyCompleteSet,
            package_price,
            max_size: max_size.unwrap_or(Decimal::ZERO),
            profit_per_set: payout - package_price,
            signature: opportunity_signature(ArbitrageKind::BuyCompleteSet, &legs),
            legs,
        })
    }

    fn sell_complete_set_opportunity(&self) -> Option<ArbitrageOpportunity> {
        let mut package_price = Decimal::ZERO;
        let mut max_size: Option<Decimal> = None;
        let mut legs = Vec::new();

        for outcome in &self.outcomes {
            let book = self.books.get(&outcome.asset_id)?;
            let (price, size) = book.best_bid()?;
            package_price += price;
            max_size = Some(max_size.map_or(size, |current| current.min(size)));
            legs.push(ArbitrageLeg {
                outcome: outcome.label.clone(),
                asset_id: outcome.asset_id,
                side: "SELL".to_owned(),
                price: price.to_string(),
                size: size.to_string(),
            });
        }

        let payout = Decimal::from(COMPLETE_SET_PAYOUT);
        if package_price <= payout {
            return None;
        }

        Some(ArbitrageOpportunity {
            kind: ArbitrageKind::SellCompleteSet,
            package_price,
            max_size: max_size.unwrap_or(Decimal::ZERO),
            profit_per_set: package_price - payout,
            signature: opportunity_signature(ArbitrageKind::SellCompleteSet, &legs),
            legs,
        })
    }
}

fn opportunity_signature(kind: ArbitrageKind, legs: &[ArbitrageLeg]) -> String {
    let prices = legs
        .iter()
        .map(|leg| format!("{}:{}", leg.asset_id, leg.price))
        .collect::<Vec<_>>()
        .join("|");
    format!("{}:{prices}", kind.label())
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, str::FromStr};

    use polymarket_client_sdk_v2::types::U256;

    use crate::market::types::{OrderBook, OutcomeToken, PaperTrade, Trade};

    use super::*;

    #[test]
    fn detects_buy_complete_set_when_asks_sum_below_one() {
        let app = app_with_books([
            ("Yes", "0.48", "5", "0.49", "3"),
            ("No", "0.50", "4", "0.50", "7"),
        ]);

        let opportunities = app.arbitrage_opportunities();

        assert_eq!(opportunities.len(), 1);
        assert!(matches!(
            opportunities[0].kind,
            ArbitrageKind::BuyCompleteSet
        ));
        assert_eq!(opportunities[0].package_price, decimal("0.99"));
        assert_eq!(opportunities[0].profit_per_set, decimal("0.01"));
        assert_eq!(opportunities[0].max_size, decimal("3"));
    }

    #[test]
    fn detects_sell_complete_set_when_bids_sum_above_one() {
        let app = app_with_books([
            ("Yes", "0.52", "5", "0.54", "3"),
            ("No", "0.51", "4", "0.53", "7"),
        ]);

        let opportunities = app.arbitrage_opportunities();

        assert_eq!(opportunities.len(), 1);
        assert!(matches!(
            opportunities[0].kind,
            ArbitrageKind::SellCompleteSet
        ));
        assert_eq!(opportunities[0].package_price, decimal("1.03"));
        assert_eq!(opportunities[0].profit_per_set, decimal("0.03"));
        assert_eq!(opportunities[0].max_size, decimal("4"));
    }

    fn app_with_books<const N: usize>(levels: [(&str, &str, &str, &str, &str); N]) -> AppState {
        let outcomes: Vec<OutcomeToken> = levels
            .iter()
            .enumerate()
            .map(|(index, (label, _, _, _, _))| OutcomeToken {
                label: (*label).to_owned(),
                asset_id: U256::from(index + 1),
            })
            .collect();
        let asset_id = outcomes[0].asset_id;
        let mut books = BTreeMap::new();

        for (index, (_, bid, bid_size, ask, ask_size)) in levels.iter().enumerate() {
            let mut book = OrderBook::default();
            book.bids.insert(decimal(bid), decimal(bid_size));
            book.asks.insert(decimal(ask), decimal(ask_size));
            books.insert(U256::from(index + 1), book);
        }

        AppState {
            slug: "market-slug".to_owned(),
            question: "Question?".to_owned(),
            outcome: outcomes[0].label.clone(),
            asset_id,
            outcomes: outcomes.clone(),
            bids: books.get(&asset_id).unwrap().bids.clone(),
            asks: books.get(&asset_id).unwrap().asks.clone(),
            books,
            last_latency_ms: 0,
            trades: VecDeque::<Trade>::new(),
            paper_trade: PaperTrade::new("market-slug", "Question?", &outcomes, Decimal::from(100)),
            tick_size: decimal("0.01"),
            scroll: 0,
            last_ts: String::new(),
        }
    }

    fn decimal(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }
}
