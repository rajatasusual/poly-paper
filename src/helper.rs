// ── HELPERS ─────────────────────────────────────
use std::io::{self, Write};

use anyhow::Result;

use polymarket_client_sdk_v2::gamma::{
    Client as gammaClient,
    types::{
        request::{MarketBySlugRequest, SearchRequest},
        response::{Event as GammaEvent, Market},
    },
};

use crate::types::*;

pub async fn resolve_market(slug: &str) -> Result<Market> {
    let client = gammaClient::default();
    Ok(client
        .market_by_slug(&MarketBySlugRequest::builder().slug(slug).build())
        .await?)
}

pub fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

pub fn has_clob_tokens(market: &Market) -> bool {
    market
        .clob_token_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty())
}

pub fn event_title(event: &GammaEvent) -> String {
    event
        .title
        .as_deref()
        .or(event.slug.as_deref())
        .unwrap_or("untitled event")
        .to_owned()
}

pub fn market_label(market: &Market) -> String {
    let question = market
        .question
        .as_deref()
        .or(market.slug.as_deref())
        .unwrap_or("untitled market");
    let slug = market.slug.as_deref().unwrap_or("no-slug");

    format!("{question} [{slug}]")
}

pub async fn search_event_page(query: &str, page: i32) -> Result<EventSearchPage> {
    let client = gammaClient::default();
    let results = client
        .search(
            &SearchRequest::builder()
                .q(query)
                .events_status("active".to_string())
                .limit_per_type(SEARCH_PAGE_SIZE)
                .page(page)
                .keep_closed_markets(0)
                .search_tags(false)
                .search_profiles(false)
                .build(),
        )
        .await?;

    let mut choices = Vec::new();
    for event in results.events.unwrap_or_default() {
        let markets: Vec<Market> = event
            .markets
            .as_ref()
            .into_iter()
            .flatten()
            .filter(|market| !market.closed.unwrap_or(false) && has_clob_tokens(market))
            .cloned()
            .collect();

        if markets.is_empty() {
            continue;
        }

        choices.push(EventChoice {
            label: event_title(&event),
            markets,
        });
    }

    let has_more = results
        .pagination
        .and_then(|pagination| pagination.has_more)
        .unwrap_or(false);

    Ok(EventSearchPage {
        choices,
        page,
        has_more,
    })
}

pub fn print_event_page(query: &str, page: &EventSearchPage) {
    println!();
    println!("Events for \"{query}\" - page {}", page.page);
    for (i, choice) in page.choices.iter().enumerate() {
        println!(
            "{:>2}. {} ({} markets)",
            i + 1,
            choice.label,
            choice.markets.len()
        );
    }
    println!();
}

pub fn print_market_page(event: &EventChoice, page: usize) {
    let start = page * MARKET_PAGE_SIZE;
    let end = (start + MARKET_PAGE_SIZE).min(event.markets.len());

    println!();
    println!(
        "Markets for \"{}\" - page {}",
        event.label,
        page.saturating_add(1)
    );
    for (i, market) in event.markets[start..end].iter().enumerate() {
        println!("{:>2}. {}", i + 1, market_label(market));
    }
    println!();
}
