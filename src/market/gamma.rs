use anyhow::Result;
use polymarket_client_sdk_v2::gamma::{
    Client as GammaClient,
    types::{
        request::{MarketBySlugRequest, SearchRequest},
        response::{Event as GammaEvent, Market},
    },
};

use crate::market::types::{EventChoice, EventSearchPage, SEARCH_PAGE_SIZE};

pub async fn resolve_market(slug: &str) -> Result<Market> {
    let client = GammaClient::default();
    Ok(client
        .market_by_slug(&MarketBySlugRequest::builder().slug(slug).build())
        .await?)
}

pub async fn search_event_page(query: &str, page: i32) -> Result<EventSearchPage> {
    let client = GammaClient::default();
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

fn has_clob_tokens(market: &Market) -> bool {
    market
        .clob_token_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty())
}

fn event_title(event: &GammaEvent) -> String {
    event
        .title
        .as_deref()
        .or(event.slug.as_deref())
        .unwrap_or("untitled event")
        .to_owned()
}
