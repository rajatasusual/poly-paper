use std::{collections::{BTreeMap, VecDeque}, str::FromStr};

use anyhow::{Context, Result};
use polymarket_client_sdk_v2::{gamma::types::response::Market, types::{Decimal, U256}};

use crate::{helper::*, types::*};



pub fn prompt_for_event_market(event: &EventChoice) -> Result<MarketPickerResult> {
    let mut page = 0;

    loop {
        print_market_page(event, page);

        let selection = prompt_line("Select market, n/p page, b events, q query, x quit: ")?;
        match selection.as_str() {
            "" | "b" => return Ok(MarketPickerResult::Back),
            "q" => return Ok(MarketPickerResult::Query),
            "x" => return Ok(MarketPickerResult::Quit),
            "n" => {
                if (page + 1) * MARKET_PAGE_SIZE < event.markets.len() {
                    page += 1;
                } else {
                    println!("Already on the last market page.");
                }
            }
            "p" => {
                if page > 0 {
                    page -= 1;
                } else {
                    println!("Already on the first market page.");
                }
            }
            _ => {
                if let Ok(index) = selection.parse::<usize>() {
                    if index > 0 {
                        let market_index = page * MARKET_PAGE_SIZE + index - 1;
                        if let Some(market) = event.markets.get(market_index) {
                            return Ok(MarketPickerResult::Selected(market.clone()));
                        }
                    }
                }

                println!("Unrecognized market selection.");
            }
        }
    }
}

pub async fn prompt_for_market() -> Result<Option<Market>> {
    loop {
        let mut query = prompt_line("Search Polymarket (blank to quit): ")?;
        if query.is_empty() {
            return Ok(None);
        }

        let mut page = 1;

        loop {
            let event_page = search_event_page(&query, page).await?;
            if event_page.choices.is_empty() {
                if event_page.has_more {
                    println!(
                        "No active CLOB events on page {page} for \"{query}\". Try n for the next page or enter a new query."
                    );
                } else {
                    println!("No active CLOB events found for \"{query}\". Try another query.");
                    break;
                }
            } else {
                print_event_page(&query, &event_page);
            }

            let selection = prompt_line("Select event, n/p page, q query, x quit, or new query: ")?;
            match selection.as_str() {
                "" | "q" => break,
                "x" => return Ok(None),
                "n" => {
                    if event_page.has_more {
                        page += 1;
                    } else {
                        println!("Already on the last event page.");
                    }
                }
                "p" => {
                    if page > 1 {
                        page -= 1;
                    } else {
                        println!("Already on the first event page.");
                    }
                }
                _ => {
                    if let Ok(index) = selection.parse::<usize>() {
                        if index > 0 {
                            if let Some(event) = event_page.choices.get(index - 1) {
                                match prompt_for_event_market(event)? {
                                    MarketPickerResult::Selected(market) => {
                                        return Ok(Some(market));
                                    }
                                    MarketPickerResult::Back => {}
                                    MarketPickerResult::Query => break,
                                    MarketPickerResult::Quit => return Ok(None),
                                }
                                continue;
                            }
                        }
                    }

                    query = selection;
                    page = 1;
                }
            }
        }
    }
}

pub fn market_session(market: Market) -> Result<MarketSession> {
    let slug = market
        .slug
        .clone()
        .unwrap_or_else(|| "selected-market".to_string());
    let question = market.question.as_ref().context("no question")?.to_owned();
    let outcome = market
        .outcomes
        .as_ref()
        .and_then(|outcomes| outcomes.first())
        .cloned()
        .unwrap_or_else(|| "selected token".to_string());

    let asset_ids: Vec<U256> = market
        .clob_token_ids
        .context("no tokens")?
        .into_iter()
        .collect();
    let asset_id = *asset_ids.first().context("no token ids")?;

    Ok(MarketSession {
        app: AppState {
            slug,
            question,
            outcome,
            asset_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_latency_ms: 0,
            trades: VecDeque::new(),
            tick_size: Decimal::from_str("0.01")?,
            scroll: 0,
            last_ts: String::new(),
        },
        asset_ids,
    })
}