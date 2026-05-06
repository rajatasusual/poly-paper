use anyhow::Result;
use clap::Parser;

mod app;
mod gamma;
mod orderbook;
mod picker;
mod prompt;
mod render;
mod session;
mod types;
mod ws;

use app::run_market_view;
use gamma::resolve_market;
use picker::prompt_for_market;
use types::MarketViewExit;

// ── CLI ─────────────────────────────────────────

#[derive(Parser)]
struct Args {
    slug: Option<String>,
}

// ── MAIN ────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut next_market = if let Some(slug) = args.slug.as_deref() {
        match resolve_market(slug).await {
            Ok(market) => Some(market),
            Err(err) => {
                println!("Could not resolve slug \"{slug}\": {err}");
                None
            }
        }
    } else {
        None
    };

    loop {
        let market = if let Some(market) = next_market.take() {
            market
        } else if let Some(market) = prompt_for_market().await? {
            market
        } else {
            break;
        };

        match run_market_view(market).await? {
            MarketViewExit::Query => {}
            MarketViewExit::Quit => break,
        }
    }

    Ok(())
}
