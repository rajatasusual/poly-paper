//! Demonstrates subscribing to real-time orderbook updates via WebSocket.
//!
//! This example shows how to:
//! 1. Connect to the CLOB WebSocket API
//! 2. Subscribe to orderbook updates for multiple assets
//! 3. Process and display bid/ask updates in real-time
//!
//! Run with tracing enabled:
//! ```sh
//! RUST_LOG=info,hyper_util=off,hyper=off,reqwest=off,h2=off,rustls=off cargo run
//! ```
//!
//! Optionally log to a file:
//! ```sh
//! LOG_FILE=websocket_orderbook.log RUST_LOG=info,hyper_util=off,hyper=off,reqwest=off,h2=off,rustls=off cargo run --example websocket_orderbook --features ws,tracing
//! ```

use std::str::FromStr as _;

use futures::StreamExt as _;
use polymarket_client_sdk_v2::clob::ws::Client;
use polymarket_client_sdk_v2::types::U256;
use tracing::{debug, error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
tracing_subscriber::fmt::init();

    let client = Client::default();
    info!(endpoint = "websocket", "connected to CLOB WebSocket API");

    let asset_ids = vec![
        U256::from_str(
            "110773438818634620295371207310728130400076010146811232773388413858961276770493",
        )?,
        U256::from_str(
            "74063275106135071105219659043951780635625785001137455153018995478729494307067",
        )?,
    ];

    let stream = client.subscribe_orderbook(asset_ids.clone())?;
    let mut stream = Box::pin(stream);
    info!(
        endpoint = "subscribe_orderbook",
        asset_count = asset_ids.len(),
        "subscribed to orderbook updates"
    );

    while let Some(book_result) = stream.next().await {
        match book_result {
            Ok(book) => {
                info!(
                    endpoint = "orderbook",
                    asset_id = %book.asset_id,
                    market = %book.market,
                    timestamp = %book.timestamp,
                    bids = book.bids.len(),
                    asks = book.asks.len()
                );

                for (i, bid) in book.bids.iter().take(5).enumerate() {
                    debug!(
                        endpoint = "orderbook",
                        side = "bid",
                        rank = i + 1,
                        size = %bid.size,
                        price = %bid.price
                    );
                }

                for (i, ask) in book.asks.iter().take(5).enumerate() {
                    debug!(
                        endpoint = "orderbook",
                        side = "ask",
                        rank = i + 1,
                        size = %ask.size,
                        price = %ask.price
                    );
                }

                if let Some(hash) = &book.hash {
                    debug!(endpoint = "orderbook", hash = %hash);
                }
            }
            Err(e) => error!(endpoint = "orderbook", error = %e),
        }
    }

    Ok(())
}