use anyhow::Result;
use clap::Parser;
use regex::Regex;

mod analyst;
mod market;

use market::app::run_market_view;
use market::gamma::resolve_market;
use market::picker::prompt_for_market;
use market::types::MarketViewExit;

// ── CLI ─────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    Analyst {
        logs: Option<String>,
    },

    Market {
        slug: Option<String>,
        series: Option<i16>,
    },
}

// - HELPER FUNCTIONS ───────────────────────────────
//Hack to get the next market slug based on the current one and the series increment. Assumes slugs are in the format "prefix-timestamp".
// Waiting for gamma to provide a more robust way to do this, but this should work for now.
fn next_market_slug(slug: &str, series: i16) -> Option<String> {
    let re = Regex::new(r"^(.*)-(\d+)$").ok()?;
    let captures = re.captures(slug)?;
    let prefix = captures.get(1)?.as_str();
    let timestamp: i64 = captures.get(2)?.as_str().parse().ok()?;
    let next_timestamp = timestamp + i64::from(series);

    Some(format!("{prefix}-{next_timestamp}"))
}

async fn run_market_mode(slug: Option<String>, series: Option<i16>) -> Result<()> {
    let mut next_market = if let Some(slug) = slug.as_deref() {
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
        let current_slug = market.slug.clone();

        match run_market_view(market).await? {
            MarketViewExit::Query => {}
            MarketViewExit::MarketClosed => {
                if let Some(current_slug) = current_slug.as_deref() {
                    println!("Market \"{current_slug}\" is closed.");

                    if let Some(next_slug) = next_market_slug(current_slug, series.unwrap()) {
                        match resolve_market(next_slug.as_str()).await {
                            Ok(market) => next_market = Some(market),
                            Err(err) => println!("Could not resolve next market: {err}"),
                        }
                    }
                } else {
                    println!("Market is closed.");
                }
            }
            MarketViewExit::Quit => break,
        }
    }

    Ok(())
}

// ── MAIN ────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Analyst { logs }) => {
            analyst::run(logs).await?;
        }

        Some(Commands::Market { slug, series }) => {
            run_market_mode(slug, series).await?;
        }

        None => {
            run_market_mode(None, None).await?;
        }
    }

    Ok(())
}
