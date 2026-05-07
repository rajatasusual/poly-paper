# Architecture

`poly-paper` market module is a small async Rust terminal app with two phases:

1. Resolve a market, either from a CLI slug or from an interactive Gamma API search.
2. Open a live Ratatui order book view backed by Polymarket CLOB websocket snapshots, while running an in-memory paper arbitrage strategy.

The code is organized around keeping API access, selection prompts, live state mutation, and rendering separate. The central runtime state is `AppState` in `types.rs`.

## Module Map

| Module | Responsibility |
| --- | --- |
| analyst | Analyse the market sessions. Read here [analyst/README.md](analyst/README.md) |
| `main.rs` | CLI entry point. Parses an optional market slug, resolves it through Gamma, and loops between market search and market view. |
| `app.rs` | Runs the market view. It prepares terminal raw mode/alternate screen, starts the websocket task, polls for market close, receives book updates, handles keyboard input, calls `render`, and writes the final JSON log. |
| `gamma.rs` | Contains Gamma REST API integration: direct market lookup by slug and active event search. It filters search results down to open markets with CLOB token IDs. |
| `picker.rs` | Implements the blocking text prompts for event and market selection, including pagination and query changes. |
| `prompt.rs` | Small shared helper for printing a prompt and reading a trimmed stdin line. |
| `session.rs` | Builds a `MarketSession` from a Gamma `Market`, extracting display metadata, outcome-token mappings, CLOB token IDs, initial order book state, and paper-trade state. |
| `ws.rs` | Owns the CLOB websocket subscription and sends `BookUpdate` messages into the app loop over a Tokio channel. |
| `orderbook.rs` | Adds book-update behavior to `AppState`. It replaces bid/ask snapshots atomically, updates timestamps/latency, and detects complete-set arbitrage opportunities across all outcomes. |
| `render.rs` | Pure UI rendering for the live market view: aggregation, volume bars, table rows, header, and arbitrage tape. |
| `types.rs` | Shared structs, enums, constants, arbitrage opportunity types, and serializable paper-trade log types. |

## Runtime Flow

Startup begins in `main.rs`.

```text
main
  -> optional gamma::resolve_market(slug)
  -> picker::prompt_for_market() when no market is already selected
  -> app::run_market_view(market)
```

If a slug is provided, the app attempts to resolve it first. If that fails, it prints the error and falls back to interactive search. When the user exits a market view with `q`, `main` loops back into search. When the user exits with `Esc` or `Ctrl-C`, the process ends. When Gamma reports the market as closed, the view exits and the app returns to the search loop.

## Search And Selection

`picker.rs` is intentionally separate from the live TUI. It uses regular stdin/stdout prompts before the app enters raw terminal mode.

Search flow:

```text
picker::prompt_for_market
  -> gamma::search_event_page(query, page)
  -> print event choices
  -> prompt_for_event_market(event)
  -> return selected Market
```

`gamma::search_event_page` asks Gamma for active events, discards closed markets, and only keeps markets that include CLOB token IDs. This means the live view only receives markets that should be subscribable through the order book websocket.

## Market Session

`session::market_session` adapts a Gamma `Market` into local runtime state.

It extracts:

- Market slug for display
- Question and outcome labels
- CLOB token IDs for websocket subscription
- First CLOB token ID as the displayed order book asset
- An `OutcomeToken` mapping for each outcome/token pair
- An empty `OrderBook` for each outcome token
- Initial paper-trade state and default UI state

The current view displays the first token ID in `asset_id`, while the websocket subscribes to all token IDs from the market. `AppState::apply_book_update` stores every subscribed token book in `books`; it also mirrors the selected token into `bids` and `asks` for the existing order book renderer.

## Live View Loop

`app::run_market_view` owns the live TUI lifecycle.

```text
run_market_view
  -> session::market_session(market)
  -> spawn ws::ws_task(asset_ids, tx)
  -> spawn market-close polling task
  -> enter raw mode + alternate screen
  -> loop
       -> drain available websocket updates
       -> app.apply_book_update(update)
       -> stop if market close was observed
       -> render(frame, app, table_state)
       -> handle key input
  -> abort websocket task
  -> abort close polling task
  -> terminal guard restores terminal state
  -> write logs/<slug>.json
```

The `TerminalGuard` in `app.rs` restores raw mode and alternate screen state on exit, including early error exits after the alternate screen has been entered.

`app.rs` writes the final paper-trade log after the terminal has been restored. File names are derived from the market slug and sanitized to ASCII alphanumeric, `-`, and `_` characters.

## Order Book State

Polymarket `subscribe_orderbook` sends full snapshots for each side. `orderbook.rs` treats each update as authoritative:

- Ignore updates for asset IDs that are not part of the selected market session.
- Clear the local side for the updated token.
- Insert positive-size levels from the snapshot.
- Keep bids and asks in `BTreeMap<Decimal, Decimal>` so best bid and ask are cheap to read from the ordered keys.
- Keep one `OrderBook` per market outcome so arbitrage detection can compare all legs.

## Arbitrage Detection

`orderbook.rs` detects complete-set arbitrage from top-of-book prices across every outcome in the market.

Two strategies are supported:

- Buy complete set: if `sum(best asks) < 1`, buy one share of every outcome. The guaranteed payout is `1`, so profit per set is `1 - sum(best asks)`.
- Sell complete set: if `sum(best bids) > 1`, sell one share of every outcome. The guaranteed cost to resolve the complete set is `1`, so profit per set is `sum(best bids) - 1`.

The executable paper size is the minimum top-level size across all legs, then capped by available paper cash. `PaperTrade` tracks how much has already been executed for each unchanged price-level signature, which prevents repeatedly paper-filling the same stale opportunity.

## Paper Trade State

`types.rs` owns the serializable paper-trade model:

- `PaperTrade` stores market metadata, outcomes, cash, pending settlement payout, realized PnL, locked PnL, total PnL, and executions.
- `PaperExecution` stores one arbitrage package fill.
- `PaperExecutionLeg` stores per-outcome side, price, executed size, available size, and notional.
- `ArbitrageOpportunity` is the internal detector output consumed by `PaperTrade::execute_opportunity`.

For buy-complete-set executions, cash decreases by the package cost and `pending_settlement_payout` increases by the number of complete sets purchased. The profit is locked until market close. On `market_closed`, pending payout is moved into cash and locked PnL becomes realized PnL.

For sell-complete-set executions, the paper model records the guaranteed spread as immediately realized PnL and tracks the required complete-set collateral for the log.

## Rendering

`render.rs` reads `AppState` and draws the current frame. It does not fetch data or mutate market data beyond the table state passed in by Ratatui.

Rendering steps:

- Find best bid and best ask.
- Aggregate raw price levels by `tick_size`.
- Select visible bid/ask levels after `scroll`.
- Compute volume bars relative to the largest visible size.
- Draw the order book table, market header, status line, and complete-set arbitrage tape.

The `+` and `-` keys change `tick_size`; the next render pass re-aggregates the visible book with the new bucket size.

## Concurrency Model

The websocket task and market-close polling task run concurrently with the UI loop.

```text
ws_task --mpsc::Sender<BookUpdate>--> app loop --mutates--> AppState --rendered by--> render
close poller --mpsc::Sender<()>------> app loop
```

The websocket task does not own or mutate `AppState`. It only forwards `BookUpdate` values. The close poller only sends a close signal. This keeps shared mutable state out of the async boundary and makes the UI loop the single writer for market and paper-trade state.

## Error Handling

Most fallible operations return `anyhow::Result`. User-facing recoverable failures, such as an unresolved initial slug, are printed and the app continues into search. Runtime failures inside the market view bubble out to `main`, while terminal cleanup is handled by `TerminalGuard`.

## Extension Points

Good places to extend the app:

- Add multi-outcome switching in `app.rs` and `session.rs` by tracking which CLOB token ID is active.
- Add a real trade stream in a new websocket module or an expanded `ws.rs` if actual prints need to be displayed separately from paper arbitrage executions.
- Add deeper tests around multi-level order book execution instead of only top-of-book paper sizing.
- Add rendering tests for fixed-width table behavior in `render.rs`.
