# poly-paper

[![CI](https://github.com/rajatasusual/poly-paper/actions/workflows/ci.yml/badge.svg)](https://github.com/rajatasusual/poly-paper/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/rust-2024-orange)
![TUI](https://img.shields.io/badge/interface-terminal-blue)
![Polymarket](https://img.shields.io/badge/data-Polymarket-green)

A terminal-native Polymarket order book viewer, complete-set arbitrage simulator, and execution analytics system.

`poly-paper` streams live Polymarket CLOB markets into a Ratatui interface, continuously evaluates complete-set arbitrage conditions across all outcomes, executes paper trades in memory, and persists structured execution logs for later analysis.

The system is composed of two major modules:

1. **Live Trading Runtime**
   - Market discovery
   - Order book streaming
   - Arbitrage detection
   - Paper execution engine
   - Session logging

2. **Post-Trade Analytics (`analyst`)**
   - Session replay
   - Execution analytics
   - Capital deployment visualization
   - Edge quality analysis
   - Real-time log monitoring

---

# Screenshot

![poly-paper order book TUI screenshot placeholder](docs/screenshot.png)

---

# System Architecture

```text
                                ┌─────────────────────┐
                                │   Polymarket APIs   │
                                │  Gamma + CLOB WS    │
                                └──────────┬──────────┘
                                           │
                     ┌─────────────────────┴─────────────────────┐
                     ▼                                           ▼
          ┌─────────────────────┐                    ┌─────────────────────┐
          │   Market Discovery  │                    │  Order Book Stream  │
          │   Search + Paging   │                    │ Incremental Updates │
          └──────────┬──────────┘                    └──────────┬──────────┘
                     │                                           │
                     └─────────────────────┬─────────────────────┘
                                           ▼
                                ┌─────────────────────┐
                                │   In-Memory Books   │
                                │ Multi-Outcome State │
                                └──────────┬──────────┘
                                           │
                                           ▼
                                ┌─────────────────────┐
                                │ Arbitrage Engine    │
                                │ Complete Set Logic  │
                                └──────────┬──────────┘
                                           │
                                           ▼
                                ┌─────────────────────┐
                                │ Paper Execution     │
                                │ Position Tracking   │
                                │ Cash Accounting     │
                                └──────────┬──────────┘
                                           │
                                           ▼
                                ┌─────────────────────┐
                                │ Session Logger      │
                                │ logs/*.json         │
                                └──────────┬──────────┘
                                           │
                                           ▼
                                ┌─────────────────────┐
                                │ Analyst TUI         │
                                │ Session Analytics   │
                                │ Metrics + Charts    │
                                └─────────────────────┘
```

---

# Core Capabilities

## Live Market Discovery

- Search active Polymarket events
- Browse paginated market lists
- Open markets directly by slug
- Fallback from invalid slug to interactive search

---

## Live CLOB Streaming

Streams websocket order book updates for all market outcomes simultaneously.

Tracks:

- bids
- asks
- spread
- cumulative depth
- tick size
- timestamps
- aggregated price levels

The runtime maintains synchronized in-memory books across every outcome in the market to support complete-set calculations.

---

## Complete-Set Arbitrage Engine

The arbitrage engine continuously evaluates top-of-book prices across all outcomes.

### Buy Complete Set

```text
sum(best asks) < 1
```

Interpretation:

Buy one share of every outcome for less than the guaranteed settlement value of `1`.

---

### Sell Complete Set

```text
sum(best bids) > 1
```

Interpretation:

Sell one share of every outcome for more than the guaranteed settlement obligation of `1`.

---

## Execution Sizing

Execution size is constrained by:

```text
min(top_level_liquidity)
AND
available paper cash
```

The engine also prevents duplicate executions against unchanged book states.

---

## Paper Execution Engine

The paper trader maintains:

- virtual cash
- open settlement exposure
- execution history
- realized PnL
- total PnL
- collateral deployment

No live orders are submitted.

The engine is strictly simulation-only.

---

## Session Logging

Each completed market session is persisted as structured JSON:

```text
logs/<market-slug>.json
```

Logs contain:

- market metadata
- outcomes
- execution history
- position state
- cash balances
- realized PnL
- settlement exposure
- per-leg trade information

These logs become the input dataset for the analytics subsystem.

---

# Analyst Subsystem

`poly-paper` includes a dedicated analytics TUI under the `analyst` module.

The analyst runtime transforms execution logs into a navigable operational analytics dashboard.

## Analyst Features

- Session browsing
- Live filesystem reload
- Session filtering
- Capital deployment visualization
- Edge quality visualization
- Execution flow inspection
- Derived metrics computation

---

## Analyst Architecture

```text
logs/*.json
      │
      ▼
┌───────────────┐
│ loader.rs     │
│ JSON ingest   │
└──────┬────────┘
       ▼
┌───────────────┐
│ models.rs     │
│ Data contracts│
└──────┬────────┘
       ▼
┌───────────────┐
│ metrics.rs    │
│ Derived stats │
└──────┬────────┘
       ▼
┌───────────────┐
│ app.rs        │
│ State + UI    │
└──────┬────────┘
       ▼
┌───────────────┐
│ watcher.rs    │
│ Live reload   │
└───────────────┘
```

---

## Analyst Dashboard

The analytics UI contains:

| Component | Purpose |
|---|---|
| Session List | Browse/filter sessions |
| Summary Panel | PnL, runtime, efficiency |
| Capital Timeline | Capital deployment sparkline |
| Edge Distribution | Trade quality visualization |
| Execution Table | Per-execution inspection |

---

## Analyst Runtime Flow

```text
JSON logs
    ↓
Deserialization
    ↓
Session models
    ↓
Filtering
    ↓
Metric computation
    ↓
Terminal rendering
```

---

# Features

## Trading Runtime

- Search active Polymarket events from the terminal
- Browse event markets with pagination
- Stream Polymarket websocket order books
- Aggregate and visualize order book depth
- Adjust aggregation tick size dynamically
- Detect complete-set arbitrage opportunities
- Execute paper trades in memory
- Poll Gamma for market closure
- Finalize PnL on market settlement
- Persist structured execution logs

---

## Analytics Runtime

- Browse historical sessions
- Live-reload filesystem changes
- Filter sessions interactively
- Visualize capital deployment
- Visualize edge quality
- Inspect execution-level flows
- Compute derived performance metrics

---

# Runtime Model

The system is event-driven.

## Trading Runtime

```text
Websocket Updates
        ↓
Order Book Mutation
        ↓
Arbitrage Evaluation
        ↓
Paper Execution
        ↓
State Update
        ↓
Render
```

---

## Analytics Runtime

```text
Filesystem Events
        ↓
Reload Sessions
        ↓
Recompute Metrics
        ↓
Render Dashboard
```

---

# Requirements

- Rust toolchain with Cargo
- Network access to:
  - Polymarket Gamma API
  - Polymarket CLOB websocket APIs

---

# Run

## Interactive Mode

```sh
cargo run
```

---

## Open Market by Slug

```sh
cargo run -- <market-slug>
```

If the slug cannot be resolved, the application falls back to interactive search.

---

## Run Analyst Dashboard

```sh
cargo run -- analyst
```

Or directly against a logs directory:

```sh
cargo run -- analyst ./logs
```

---

# Search Controls

When searching:

| Input | Action |
| --- | --- |
| text | Search for events |
| number | Select event or market |
| `n` | Next page |
| `p` | Previous page |
| `q` or blank | Back to search |
| `b` | Back from market picker |
| `x` | Quit |

---

# Market View Controls

| Key | Action |
| --- | --- |
| `Up` / `Down` | Scroll order book |
| `+` | Double aggregation tick |
| `-` | Halve aggregation tick |
| `q` | Leave current market |
| `Esc` or `Ctrl-C` | Quit |

---

# Analyst Controls

| Key | Action |
| --- | --- |
| `↑ ↓` | Navigate sessions |
| `/` | Enter filter mode |
| `ESC` | Exit filter mode |
| `r` | Reload logs |
| `q` | Quit analyst |

---

# Logs

Market sessions are stored under:

```text
logs/<market-slug>.json
```

Each session contains:

- metadata
- outcomes
- execution history
- paper trade state
- cash balances
- realized/total PnL
- settlement exposure
- execution legs

---

# Development

## Build

```sh
cargo build
```

---

## Format

```sh
cargo fmt --check
```

---

## Clippy

```sh
cargo clippy
```

---

# Design Characteristics

| Property | Approach |
|---|---|
| Interface | Terminal-native |
| Runtime Model | Event-driven |
| Trading Engine | In-memory paper execution |
| Market Data | Live websocket |
| Analytics | Post-trade TUI |
| State Model | Centralized |
| Rendering | Immediate mode |
| Concurrency | Channel-based |
| Persistence | JSON logs |

---

# Notes

This project uses live Polymarket market data.

Observed arbitrage opportunities are derived from local order book snapshots and do not account for:

- exchange fees
- execution latency
- slippage
- partial fills beyond visible liquidity
- rejected orders
- market impact

The system is intended for simulation, analytics, and strategy development workflows.