use anyhow::Result;
use polymarket_client_sdk_v2::gamma::types::response::Market;

use crate::{
    gamma::search_event_page,
    prompt::prompt_line,
    types::{EventChoice, EventSearchPage, MARKET_PAGE_SIZE, MarketPickerResult},
};

pub async fn prompt_for_market() -> Result<Option<Market>> {
    loop {
        println!();
        println!("Search");
        println!("  Enter keywords for an active Polymarket event. Leave blank to quit.");
        let mut query = prompt_line("> ")?;
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

            println!("Commands: number select | n next | p previous | q new search | x quit");
            let selection = prompt_line("> ")?;
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
                    if let Ok(index) = selection.parse::<usize>()
                        && index > 0
                        && let Some(event) = event_page.choices.get(index - 1)
                    {
                        match prompt_for_event_market(event)? {
                            MarketPickerResult::Selected(market) => {
                                return Ok(Some(*market));
                            }
                            MarketPickerResult::Back => {}
                            MarketPickerResult::Query => break,
                            MarketPickerResult::Quit => return Ok(None),
                        }
                        continue;
                    }

                    query = selection;
                    page = 1;
                }
            }
        }
    }
}

fn prompt_for_event_market(event: &EventChoice) -> Result<MarketPickerResult> {
    let mut page = 0;

    loop {
        print_market_page(event, page);

        println!("Commands: number select | n next | p previous | b events | q search | x quit");
        let selection = prompt_line("> ")?;
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
                if let Ok(index) = selection.parse::<usize>()
                    && index > 0
                {
                    let market_index = page * MARKET_PAGE_SIZE + index - 1;
                    if let Some(market) = event.markets.get(market_index) {
                        return Ok(MarketPickerResult::Selected(Box::new(market.clone())));
                    }
                }

                println!("Unrecognized market selection.");
            }
        }
    }
}

fn print_event_page(query: &str, page: &EventSearchPage) {
    println!();
    println!("Events");
    println!("  Query: \"{query}\"");
    println!("  Page: {}", page.page);
    println!();
    for (i, choice) in page.choices.iter().enumerate() {
        println!(
            "  {:>2}. {:<72} {:>3} markets",
            i + 1,
            truncate(&choice.label, 72),
            choice.markets.len()
        );
    }
    println!();
}

fn print_market_page(event: &EventChoice, page: usize) {
    let start = page * MARKET_PAGE_SIZE;
    let end = (start + MARKET_PAGE_SIZE).min(event.markets.len());

    println!();
    println!("Markets");
    println!("  Event: {}", event.label);
    println!("  Page:  {}", page.saturating_add(1));
    println!();
    for (i, market) in event.markets[start..end].iter().enumerate() {
        println!("  {:>2}. {}", i + 1, market_label(market));
    }
    println!();
}

fn market_label(market: &Market) -> String {
    let question = market
        .question
        .as_deref()
        .or(market.slug.as_deref())
        .unwrap_or("untitled market");
    let slug = market.slug.as_deref().unwrap_or("no-slug");

    format!("{question} [{slug}]")
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let mut truncated: String = value.chars().take(max_chars.saturating_sub(3)).collect();
    truncated.push_str("...");
    truncated
}
