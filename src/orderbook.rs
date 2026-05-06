use std::{collections::BTreeMap, time::Instant};

use polymarket_client_sdk_v2::{clob::ws::BookUpdate, types::Decimal};

use crate::types::{AppState, Trade, TradeSide};

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
        if update.asset_id != self.asset_id {
            return;
        }

        let start = Instant::now();
        let old_bid = self.bids.keys().next_back().cloned();
        let old_ask = self.asks.keys().next().cloned();

        replace_levels(
            &mut self.bids,
            update
                .bids
                .into_iter()
                .map(|level| (level.price, level.size)),
        );
        replace_levels(
            &mut self.asks,
            update
                .asks
                .into_iter()
                .map(|level| (level.price, level.size)),
        );

        self.detect_trade(old_bid, old_ask);
        self.last_latency_ms = start.elapsed().as_millis();
        self.last_ts = update.timestamp.to_string();
    }

    // This is a rough visual signal based on top-of-book movement, not a real trade feed.
    fn detect_trade(&mut self, old_bid: Option<Decimal>, old_ask: Option<Decimal>) {
        let new_bid = self.bids.keys().next_back().cloned();
        let new_ask = self.asks.keys().next().cloned();

        if let (Some(ob), Some(nb)) = (old_bid, new_bid)
            && nb < ob
        {
            self.trades.push_front(Trade {
                price: nb,
                size: Decimal::from(1),
                side: TradeSide::Sell,
            });
        }

        if let (Some(oa), Some(na)) = (old_ask, new_ask)
            && na > oa
        {
            self.trades.push_front(Trade {
                price: na,
                size: Decimal::from(1),
                side: TradeSide::Buy,
            });
        }

        if self.trades.len() > 20 {
            self.trades.pop_back();
        }
    }
}
