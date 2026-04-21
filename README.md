# Keiri

Keiri is a zero-dependency Rust Yahtzee oracle MVP. It provides a rules-perfect
solitaire Yahtzee core, deterministic transition engine, legal action generation,
and an exact dynamic-programming advisor for bounded states.

## Documentation

Full documentation is in the [`docs/`](docs/) directory:

- [Documentation Index](docs/index.md) — Start here for an overview and navigation
- [Architecture Overview](docs/architecture.md) — System design, component diagram, data flow
- [Agent System](docs/agents.md) — Heuristic, hybrid, exact table, and optimal agents
- [State Format](docs/state-format.md) — Canonical dice, GameState encoding, binary tables
- [CLI Reference](docs/cli-reference.md) — All commands with usage and examples
- [Public API](docs/api-reference.md) — Library types and functions
- [Ruleset Reference](docs/ruleset.md) — Categories, scoring, Joker rules
- [Oracle Tables](docs/oracle.md) — OptimalAgent, memoization, table formats
- [BuddyBoardGames](docs/buddyboardgames.md) — Web game integration

## Quick Start

```bash
# Play one solitaire game
cargo run -- simulate

# Get advice for a game state
cargo run -- advise dice=1,2,3,4,6 rolls=3

# Score a specific roll in a category
cargo run -- score full-house 2,2,3,3,3

# List legal actions for a state
cargo run -- actions dice=1,2,3,4,5 rolls=2 scores=ones:3

# Build the exact table for production-grade advice
cargo run -- build-anchor-table rules=buddyboardgames out=target/keiri_tables/bbg-anchor-v1.bin threads=auto builder=dense

# Run many simulation games
cargo run -- evaluate games=10000 seed=1 out=metrics/simulation_history.csv
```

## Commands

All commands use `key=value` argument tokens. See the [CLI Reference](docs/cli-reference.md) for full details.

### Simulation

```bash
cargo run -- --simulate seed=42 verbose=true
cargo run -- --simulate rules=buddyboardgames agent=auto table=target/keiri_tables/bbg-anchor-v1.bin seed=42
```

- `simulate` — Play one solitaire game (outputs final score)
- `evaluate` — Run many games and output summary metrics
- `bbg-loop` — Grind solo BuddyBoardGames games indefinitely

### BuddyBoardGames

```bash
cargo run -- --bbg-join my-room-code
cargo run -- bbg-join room=my-room-code player=Keiri play=true
```

See [BuddyBoardGames docs](docs/buddyboardgames.md) for full integration details.

### Advisor

```bash
cargo run -- actions dice=1,2,3,4,5 rolls=2 scores=ones:3,twos:6
cargo run -- advise dice=1,2,3,4,6 rolls=3 scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50
cargo run -- bbg-advise state=STARTED me=0 turn=0 spectator=false pending=false dice=1,2,3,4,5 selected=0,0,0,0,0 rolls=2 rows=0:3:1,1:6:1
```

- `score` — Compute score for a category and dice roll
- `actions` — List legal actions for a state
- `advise` — Get recommended action (uses agent specified by `--agent`)

### Tables

```bash
cargo run -- build-table out=target/keiri_tables/chance.tsv depths=3 scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50
cargo run -- build-anchor-table rules=buddyboardgames out=target/keiri_tables/bbg-anchor-v1.bin threads=auto builder=dense
```

- `build-table` — Build OracleTable (TSV, endgame slices)
- `build-anchor-table` — Build AnchorValueTable (binary, full game)

## Agent Selection

| Agent | Quality | Use Case |
|-------|---------|----------|
| `heuristic` | ~75-85% optimal | Fast comparisons, no table needed |
| `hybrid` | ~85-95% optimal | Middle ground, no table needed |
| `auto` (default) | Near-optimal | Production, uses exact table |
| `optimal` | Exact | Verification, benchmarking |

Select with `agent=<name>` or `--agent <name>`. See [Agent System](docs/agents.md) for details.

## Verification

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo bench
```

## Scope

This repository currently implements the Rust Oracle MVP, full-game simulation,
bounded table generation, exact anchor-table generation, and a guarded
BuddyBoardGames adapter. Dataset generation and student policy models remain
future phases.
