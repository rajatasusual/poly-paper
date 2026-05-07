# Architecture

`poly-paper` analyst module is a terminal-based analytics dashboard for inspecting trading or execution sessions from JSON logs in real time.

Built with:

- [Ratatui](https://ratatui.rs) for terminal UI rendering
- [Crossterm](https://crates.io/crates/crossterm) for terminal/event handling
- [Notify](https://crates.io/crates/notify) for filesystem watching
- [Serde JSON](https://crates.io/crates/serde_json) for session deserialization

---

# Purpose

This module provides a live operational analytics interface over a directory of session JSON files.

It is designed around:

- Continuous log ingestion
- Lightweight in-terminal observability
- Real-time session inspection
- Fast filtering/navigation workflows
- Derived analytics from execution-level data

The architecture separates:

- data ingestion
- metric computation
- filesystem watching
- application state
- rendering/event loop

---

# High-Level Architecture

```text
                 ┌─────────────────────┐
                 │ JSON Session Logs   │
                 │   ./logs/*.json     │
                 └─────────┬───────────┘
                           │
                           ▼
                 ┌─────────────────────┐
                 │      loader.rs      │
                 │ load_sessions()     │
                 └─────────┬───────────┘
                           │
                           ▼
                 ┌─────────────────────┐
                 │      models.rs      │
                 │ Session Structures  │
                 └─────────┬───────────┘
                           │
                           ▼
                 ┌─────────────────────┐
                 │       app.rs        │
                 │ Application State   │
                 │ Filtering           │
                 │ Navigation          │
                 │ Rendering           │
                 └─────────┬───────────┘
                           │
            ┌──────────────┴──────────────┐
            ▼                             ▼
 ┌─────────────────────┐      ┌─────────────────────┐
 │     metrics.rs      │      │     watcher.rs      │
 │ Derived Analytics   │      │ Filesystem Watcher  │
 └─────────────────────┘      └─────────────────────┘
                           │
                           ▼
                 ┌─────────────────────┐
                 │      mod.rs         │
                 │ TUI Runtime Loop    │
                 │ Keyboard Events     │
                 │ Terminal Lifecycle  │
                 └─────────────────────┘
```

---

# Runtime Flow

## 1. Startup

Entry point:

```rust
pub async fn run() -> Result<()>
```

Flow:

1. Resolve logs directory from CLI args
2. Initialize `App`
3. Enable terminal raw mode
4. Switch to alternate screen
5. Start render/event loop

```text
CLI args
   ↓
App::new()
   ↓
load_sessions()
   ↓
watch_logs()
   ↓
run_app()
```

Default logs directory:

```text
./logs
```

Custom path:

```bash
cargo run -- ./custom_logs
```

---

# Core Components

# `mod.rs` — Runtime + Event Loop

Responsible for:

- terminal initialization
- terminal restoration
- keyboard input handling
- render loop orchestration
- periodic polling

Main loop:

```text
loop:
    app.tick()
    render()
    poll keyboard input
    mutate app state
```

### Supported Keybindings

| Key | Action |
|---|---|
| `↑ ↓` | Navigate sessions |
| `/` | Enter filter mode |
| `ESC` | Exit filter mode |
| `r` | Reload logs |
| `q` | Quit |
| `Backspace` | Edit filter |
| `text input` | Live filter |

---

# `app.rs` — Application State + UI Composition

`App` is the central state container.

## Responsibilities

### State Management

Tracks:

- loaded sessions
- filtered indices
- active selection
- filter state
- reload events

### UI Rendering

Composes the dashboard into regions:

```text
┌──────────────────────────────────────────────┐
│ Session List │ Dashboard                    │
│               │                              │
│               │ Summary                      │
│               │ Capital Timeline             │
│               │ Edge Distribution            │
│               │ Execution Table              │
├──────────────────────────────────────────────┤
│ Footer / Keyboard Shortcuts                 │
└──────────────────────────────────────────────┘
```

### Session Navigation

Maintains stable cursor movement through:

```rust
filtered_sessions: Vec<usize>
```

This avoids cloning session data during filtering.

### Filtering

Filtering is performed against:

- `session.slug`
- `session.question`

Filtering pipeline:

```text
User Input
    ↓
filter_input
    ↓
apply_filter()
    ↓
filtered_sessions
    ↓
rendered list
```

### Live Reload

`tick()` checks the watcher channel:

```rust
reload_rx.try_recv()
```

If filesystem changes occur:

```text
watcher.rs
   ↓
reload signal
   ↓
App::tick()
   ↓
reload()
   ↓
load_sessions()
```

---

# Dashboard Rendering Flow

## Session Summary

Displays derived metrics:

- realized PnL
- execution count
- runtime duration
- peak deployment
- efficiency ratio
- cash balance

Metrics are computed dynamically via:

```rust
compute_metrics(session)
```

---

## Capital Deployment Timeline

Visualizes:

```text
pending_settlement_payout_after
```

across executions using a sparkline.

Purpose:

- observe deployment growth
- identify leverage spikes
- detect capital compression

---

## Edge Quality Distribution

Visualizes:

```text
guaranteed_profit
```

across executions.

Purpose:

- inspect execution quality
- identify edge decay
- compare strategy consistency

---

## Execution Flow Table

Displays per-execution data:

| Field | Meaning |
|---|---|
| Strategy | Execution strategy name |
| Size | Position size |
| Package | Package price |
| Edge | Guaranteed profit |

Rows are colorized:

| Condition | Color |
|---|---|
| `edge > 0.25` | Green |
| `0.10 < edge <= 0.25` | Yellow |
| `edge <= 0.10` | Red |

---

# `loader.rs` — Session Ingestion

Responsible for filesystem ingestion.

## Responsibilities

- create logs directory if missing
- scan for `.json` files
- deserialize sessions
- sort chronologically

Pipeline:

```text
Filesystem
   ↓
read_dir()
   ↓
.json filter
   ↓
read_to_string()
   ↓
serde_json::from_str()
   ↓
Vec<Session>
```

Sessions are sorted using:

```rust
started_unix_ms
```

This guarantees deterministic ordering.

---

# `metrics.rs` — Derived Analytics

Pure computation layer.

No UI logic.

No filesystem logic.

## Current Metrics

### PnL

```text
realized_pnl
```

### Duration

```text
ended_unix_ms - started_unix_ms
```

### Peak Deployment

Maximum observed:

```text
pending_settlement_payout_after
```

### Efficiency

Computed as:

```text
pnl / peak_deployment
```

This approximates capital efficiency.

---

# `models.rs` — Data Contracts

Defines the domain schema.

## Hierarchy

```text
Session
 ├── Execution[]
 │     └── Leg[]
```

## Session

Represents a complete trading/analysis run.

Contains:

- metadata
- pnl
- execution history
- capital state

## Execution

Represents an individual strategy execution.

Contains:

- pricing
- edge
- collateral
- settlement state

## Leg

Represents a sub-component of an execution.

Useful for:

- multi-leg strategies
- synthetic positions
- arbitrage decomposition

---

# `watcher.rs` — Filesystem Monitoring

Provides live reload capability.

## Architecture

```text
notify watcher
      ↓
filesystem events
      ↓
mpsc channel
      ↓
App reload signal
```

The watcher runs on a dedicated thread.

This decouples:

- filesystem IO
- UI rendering
- application state mutation

---

# Concurrency Model

The application intentionally uses a minimal concurrency architecture.

## Threads

### Main Thread

Responsible for:

- rendering
- input handling
- state mutation

### Watcher Thread

Responsible for:

- filesystem event listening
- reload signaling

Communication uses:

```rust
std::sync::mpsc
```

This avoids shared mutable state and mutex complexity.

---

# Error Handling Strategy

Uses:

```rust
anyhow::Result
```

throughout the module.

Characteristics:

- simplified propagation
- ergonomic `?` usage
- centralized failure handling

Non-critical failures are intentionally ignored in some areas:

```rust
let _ = self.reload();
```

This prevents transient reload failures from crashing the UI.

---

# Rendering Strategy

The UI is fully stateless at the rendering layer.

Each frame:

```text
state
  ↓
derived widgets
  ↓
render
```

No widget caches are maintained.

Benefits:

- deterministic rendering
- simpler state model
- reduced synchronization concerns

Tradeoff:

- full redraw each frame

Acceptable because:

- terminal rendering cost is low
- dataset size is expected to remain moderate

---

# Data Lifecycle

```text
JSON logs
    ↓
Deserialization
    ↓
Session structs
    ↓
Filtering
    ↓
Selection
    ↓
Metric computation
    ↓
Widget rendering
```

---

# Extension Points

## Additional Metrics

Add to:

```text
metrics.rs
```

Examples:

- Sharpe ratio
- drawdown
- fill latency
- slippage
- volatility

---

## Additional Charts

Add render sections in:

```text
render_dashboard()
```

Potential additions:

- latency histogram
- pnl curve
- exposure heatmap
- strategy attribution

---

## Persistent Filtering

Current filtering is ephemeral.

Can be extended with:

- regex filtering
- multi-field filtering
- saved views
- fuzzy search

---

## Streaming Ingestion

Current architecture reloads entire datasets.

Future improvement:

```text
incremental append-only ingestion
```

instead of full reloads.

---

# Design Characteristics

| Property | Approach |
|---|---|
| UI Framework | Immediate mode |
| State Model | Centralized |
| Concurrency | Channel-based |
| Rendering | Full redraw |
| Data Source | JSON filesystem |
| Reload Model | Event-driven |
| Failure Model | Best-effort |
| Filtering | In-memory |
| Metrics | Derived at render time |

---

# Example Session Schema

```json
{
  "slug": "btc-arb-001",
  "question": "Cross-exchange BTC spread",
  "started_unix_ms": 1710000000000,
  "ended_unix_ms": 1710000005000,
  "realized_pnl": "12.42",
  "total_pnl": "15.01",
  "cash": "1200.00",
  "executions": [
    {
      "strategy": "maker-taker",
      "package_price": "0.42",
      "size": "10",
      "guaranteed_profit": "0.18",
      "collateral": "100",
      "cash_after": "1100",
      "pending_settlement_payout_after": "1300",
      "market_timestamp": "1710000001000",
      "legs": []
    }
  ]
}
```

---

# Summary

This module implements a lightweight event-driven analytics terminal for session-based execution logs.

The architecture emphasizes:

- separation of concerns
- deterministic rendering
- minimal concurrency
- operational observability
- extensibility for quantitative analytics workflows