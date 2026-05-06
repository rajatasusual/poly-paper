# poly-paper

A terminal order book viewer for Polymarket markets.

`poly-paper` searches active Polymarket events, lets you pick a CLOB-backed market, and streams the selected market's order book into a Ratatui interface. It shows aggregated bid/ask levels, cumulative size, spread, update latency, and a lightweight trade tape inferred from top-of-book movement.

## Features

- Search active Polymarket events from the terminal
- Browse event markets with pagination
- Connect to Polymarket CLOB websocket order book updates
- View bids, asks, cumulative depth, spread, tick size, and update timestamp
- Adjust price aggregation while the book is live

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
