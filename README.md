# poly-paper

A terminal order book viewer for Polymarket markets.

`poly-paper` searches active Polymarket events, lets you pick a CLOB-backed market, and streams the selected market's order book into a Ratatui interface. It shows aggregated bid/ask levels, cumulative size, spread, update latency, and a lightweight trade tape inferred from top-of-book movement.

## Features

- Search active Polymarket events from the terminal
- Browse event markets with pagination
- Connect to Polymarket CLOB websocket order book updates
- View bids, asks, cumulative depth, spread, tick size, and update timestamp
- Adjust price aggregation while the book is live

## Architecture

For the full architecture notes, see [src/ARCHITECTURE.md](src/ARCHITECTURE.md).

| Module | Responsibility |
| --- | --- |
| [`src/main.rs`](src/main.rs) | Parses CLI arguments, resolves an optional slug, and drives the search/view loop. |
| [`src/app.rs`](src/app.rs) | Owns the live market TUI loop, terminal lifecycle, key handling, and websocket task wiring. |
| [`src/gamma.rs`](src/gamma.rs) | Wraps Polymarket Gamma API calls for market lookup and active event search. |
| [`src/orderbook.rs`](src/orderbook.rs) | Applies websocket book snapshots to `AppState` and maintains inferred trade events. |
| [`src/picker.rs`](src/picker.rs) | Implements the interactive event and market selection prompts. |
| [`src/prompt.rs`](src/prompt.rs) | Provides the shared stdin prompt helper. |
| [`src/render.rs`](src/render.rs) | Renders the Ratatui order book, header, status lines, volume bars, and trade tape. |
| [`src/session.rs`](src/session.rs) | Converts a Gamma market into the initial `MarketSession` and `AppState`. |
| [`src/types.rs`](src/types.rs) | Defines shared state, event/search structs, constants, and control-flow enums. |
| [`src/ws.rs`](src/ws.rs) | Subscribes to Polymarket CLOB websocket order book updates and forwards them over a channel. |

## Requirements

- Rust toolchain with Cargo
- Network access to Polymarket Gamma and CLOB websocket APIs

## Run

Start the app and search interactively:

```sh
cargo run
```

Open a market directly by slug:

```sh
cargo run -- <market-slug>
```

For example:

```sh
cargo run -- will-bitcoin-hit-100k-in-2024
```

If the slug cannot be resolved, the app falls back to interactive search.

## Search Controls

When searching:

| Input | Action |
| --- | --- |
| text | Search for events, or start a new search from the event picker |
| number | Select an event or market from the current page |
| `n` | Next page |
| `p` | Previous page |
| `q` or blank input | Back to search query |
| `b` | Back from market picker to event picker |
| `x` | Quit |

## Market View Controls

| Key | Action |
| --- | --- |
| `Up` / `Down` | Scroll visible order book levels |
| `+` | Double aggregation tick size |
| `-` | Halve aggregation tick size |
| `q` | Leave the current market and search again |
| `Esc` or `Ctrl-C` | Quit |

## Development

Build the project:

```sh
cargo build
```

Check formatting:

```sh
cargo fmt --check
```

Run Clippy:

```sh
cargo clippy
```

## Notes

This is a live market-data TUI. Displayed order book data depends on Polymarket API availability and the selected market having CLOB token IDs. The trade tape is a rough local inference from changes in the best bid and ask, not an authoritative trade feed.
