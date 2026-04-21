# Agent System

The agent system provides a unified `Agent` trait with multiple implementations
ranging from fast heuristic rules to exact optimal solvers. Each agent decides
what action to take given a `GameState`.

## The Agent Trait

```rust
pub trait Agent {
    fn select_action(&self, state: &GameState, ruleset: Ruleset) -> Decision;
}
```

The trait returns a `Decision` which contains an `Action` and optionally an
`expected_value` and `DecisionSource`. The `DecisionSource` indicates where
the decision came from (oracle, table, heuristic, or optimal solver).

## Agent Implementations

### HeuristicAgent

A rule-based agent that makes decisions without any precomputed data or
recursive search.

**How it works:**
- If dice can be scored in an open category, keeps the best-scoring subset
- Otherwise rolls to improve straight or high-value combinations
- Falls back to rolling all dice when no scoring pattern is detected

**Pros:**
- No setup cost, no disk I/O
- Decision time is negligible (microseconds)
- Always available regardless of table state

**Cons:**
- Typically 75-85% of optimal play quality
- No guarantee of best decision in any position

**When to use:**
- Quick comparisons without table overhead
- Development and debugging
- When table files are not available

**CLI selection:**
```bash
cargo run -- --simulate agent=heuristic
cargo run -- advise agent=heuristic dice=1,2,3,4,6 rolls=3
```

### HybridAgent

Combines heuristic decisions for early turns with optimal agent decisions for
late turns (last 2-3 categories remaining).

**How it works:**
- Uses `OptimalAgent` when few categories remain (high-value decisions)
- Falls back to `HeuristicAgent` for early turns where exact optimality
  matters less

**Pros:**
- Best decisions when they matter most (endgame)
- No table file needed
- Better than pure heuristic without full optimal cost

**Cons:**
- Still not exact for early turns
- `OptimalAgent` memoization cache grows over time

**When to use:**
- When you want better-than-heuristic play without table files
- A middle ground between speed and quality

**CLI selection:**
```bash
cargo run -- --simulate agent=hybrid
```

### ExactTableAgent

Looks up decisions from a precomputed `AnchorValueTable`. This is the default
for production use.

**How it works:**
- Queries the `AnchorValueTable` with the current game state
- Returns the precomputed optimal action for that state
- Table is built with `build-anchor-table` using dense dynamic programming

**Pros:**
- Near-optimal play across all game states
- Fast lookup (O(1) after table load)
- No recursive computation at decision time
- Verified opening expected value: 254.589609 (BBG ruleset)

**Cons:**
- Requires prebuilt table file (~tens of MB)
- Table build time on first use (seconds on M4 Pro)

**When to use:**
- Production advisor (`agent=auto` defaults to this)
- Any scenario requiring consistent high-quality play
- BuddyBoardGames integration

**CLI selection:**
```bash
cargo run -- --simulate agent=auto table=target/keiri_tables/bbg-anchor-v1.bin
cargo run -- advise agent=auto dice=1,2,3,4,6 rolls=3
```

### OptimalAgent

An exact recursive solver with memoization. Computes the true optimal action
for any state on demand.

**How it works:**
- Recursively explores all possible dice outcomes and actions
- Uses memoization (`HashMap<AnchorKey, f64>`) to avoid recomputation
- Bottom-up: values for fewer open categories are computed first naturally
- Each instance owns its memoization cache and can be reused across queries

**Pros:**
- Mathematically optimal decisions
- No precomputed table needed
- On-demand computation, only caches states that are queried

**Cons:**
- Slower than table lookup (recursive computation per decision)
- Memory grows with unique states encountered
- Not practical for real-time use without caching

**When to use:**
- Verifying table accuracy
- Benchmarking other agents
- Debugging decision quality
- Endgame analysis

**CLI selection:**
```bash
cargo run -- --simulate agent=optimal
```

## Agent Comparison

| Agent | Quality | Speed | Setup | Needs Table |
|-------|---------|-------|-------|-------------|
| HeuristicAgent | ~75-85% optimal | Microseconds | None | No |
| HybridAgent | ~85-95% optimal | Microseconds + memo | None | No |
| ExactTableAgent | Near-optimal | O(1) lookup | Table build (sec) | Yes |
| OptimalAgent | Exact | Recursive per query | None | No |

## Agent Selection Flow

The CLI `--agent` flag selects the implementation:

| Value | Implementation | Default For |
|-------|---------------|-------------|
| `heuristic` | HeuristicAgent | Comparison tests |
| `hybrid` | HybridAgent | Middle ground |
| `auto` | ExactTableAgent | Production, BBG |
| `optimal` | OptimalAgent | Verification |

When `agent=auto` is used and no table is specified via `--table`, Keiri uses
the default table path (`target/keiri_tables/bbg-anchor-v1.bin`). If the table
file does not exist, it is built automatically using the optimized release
binary with dense dynamic programming.

## Decision Structure

The `Decision` type returned by `select_action` contains:

- `action`: The `Action` to take (roll with hold mask, or score in a category)
- `expected_value`: Optional f64 estimate of the action's long-term outcome
- `source`: `DecisionSource` indicating where the decision originated

`DecisionSource` variants:
- `Oracle` — from `OptimalAgent` recursive solver
- `Table` — from `ExactTableAgent` via `AnchorValueTable` lookup
- `Heuristic` — from `HeuristicAgent` rule-based logic
- `Hybrid` — from `HybridAgent` (heuristic or optimal depending on state)
