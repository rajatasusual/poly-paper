# Architecture

`poly-paper` is a small async Rust terminal app with two phases:

1. Resolve a market, either from a CLI slug or from an interactive Gamma API search.
2. Open a live Ratatui order book view backed by Polymarket CLOB websocket snapshots.

The code is organized around keeping API access, selection prompts, live state mutation, and rendering separate. The central runtime state is `AppState` in `types.rs`.

## Module Map

| Module | Responsibility |
| --- | --- |
| `main.rs` | CLI entry point. Parses an optional market slug, resolves it through Gamma, and loops between market search and market view. |
| `app.rs` | Runs the market view. It prepares terminal raw mode/alternate screen, starts the websocket task, receives book updates, handles keyboard input, and calls `render`. |
| `gamma.rs` | Contains Gamma REST API integration: direct market lookup by slug and active event search. It filters search results down to open markets with CLOB token IDs. |
| `picker.rs` | Implements the blocking text prompts for event and market selection, including pagination and query changes. |
| `prompt.rs` | Small shared helper for printing a prompt and reading a trimmed stdin line. |
| `session.rs` | Builds a `MarketSession` from a Gamma `Market`, extracting display metadata, CLOB token IDs, and initial order book state. |
| `ws.rs` | Owns the CLOB websocket subscription and sends `BookUpdate` messages into the app loop over a Tokio channel. |
| `orderbook.rs` | Adds book-update behavior to `AppState`. It replaces bid/ask snapshots atomically, updates timestamps/latency, and records rough inferred trade signals. |
| `render.rs` | Pure UI rendering for the live market view: aggregation, volume bars, table rows, header, and trade tape. |
| `types.rs` | Shared structs, enums, and constants used across modules. |

## Runtime Flow

Startup begins in `main.rs`.

```text
main
  -> optional gamma::resolve_market(slug)
  -> picker::prompt_for_market() when no market is already selected
  -> app::run_market_view(market)
```

If a slug is provided, the app attempts to resolve it first. If that fails, it prints the error and falls back to interactive search. When the user exits a market view with `q`, `main` loops back into search. When the user exits with `Esc` or `Ctrl-C`, the process ends.

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
- Question and first outcome label
- CLOB token IDs for websocket subscription
- First CLOB token ID as the displayed order book asset
- Empty bid/ask maps and default UI state

The current view tracks the first token ID in `asset_id`, while the websocket subscribes to all token IDs from the market. Incoming book updates for non-selected asset IDs are ignored in `AppState::apply_book_update`.

## Live View Loop

`app::run_market_view` owns the live TUI lifecycle.

```text
run_market_view
  -> session::market_session(market)
  -> spawn ws::ws_task(asset_ids, tx)
  -> enter raw mode + alternate screen
  -> loop
       -> drain available websocket updates
       -> app.apply_book_update(update)
       -> render(frame, app, table_state)
       -> handle key input
  -> abort websocket task
  -> terminal guard restores terminal state
```

The `TerminalGuard` in `app.rs` restores raw mode and alternate screen state on exit, including early error exits after the alternate screen has been entered.

## Order Book State

Polymarket `subscribe_orderbook` sends full snapshots for each side. `orderbook.rs` treats each update as authoritative:

- Clear the local side.
- Insert positive-size levels from the snapshot.
- Keep bids and asks in `BTreeMap<Decimal, Decimal>` so best bid and ask are cheap to read from the ordered keys.

The inferred trade tape is deliberately lightweight. It compares old and new top-of-book prices and records a synthetic buy/sell marker when the best ask moves up or the best bid moves down. This is useful as a visual cue, but it is not an authoritative trade feed.

## Rendering

`render.rs` reads `AppState` and draws the current frame. It does not fetch data or mutate market data beyond the table state passed in by Ratatui.

Rendering steps:

- Find best bid and best ask.
- Aggregate raw price levels by `tick_size`.
- Select visible bid/ask levels after `scroll`.
- Compute volume bars relative to the largest visible size.
- Draw the order book table, market header, status line, and inferred trade tape.

The `+` and `-` keys change `tick_size`; the next render pass re-aggregates the visible book with the new bucket size.

## Concurrency Model

Only the websocket task runs concurrently with the UI loop.

```text
ws_task --mpsc::Sender<BookUpdate>--> app loop --mutates--> AppState --rendered by--> render
```

The websocket task does not own or mutate `AppState`. It only forwards `BookUpdate` values. This keeps shared mutable state out of the async boundary and makes the UI loop the single writer for market state.

## Error Handling

Most fallible operations return `anyhow::Result`. User-facing recoverable failures, such as an unresolved initial slug, are printed and the app continues into search. Runtime failures inside the market view bubble out to `main`, while terminal cleanup is handled by `TerminalGuard`.

## Extension Points

Good places to extend the app:

- Add multi-outcome switching in `app.rs` and `session.rs` by tracking which CLOB token ID is active.
- Replace inferred trades with a real trade stream in a new websocket module or an expanded `ws.rs`.
- Add tests around `orderbook.rs` for snapshot replacement and trade inference.
- Add rendering tests for fixed-width table behavior in `render.rs`.
