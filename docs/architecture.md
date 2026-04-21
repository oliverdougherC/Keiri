# Architecture Overview

This document covers the high-level system design of Keiri: components, data
flow, agent tiers, the table system, and key design decisions.

## Component Diagram

```
┌─────────────────────────────────────────────────────┐
│                     CLI (main.rs)                    │
│  simulate | bbg-join | bbg-loop | evaluate | score  │
│  actions | advise | bbg-advise | build-table        │
│  build-anchor-table                                 │
└──────────────────────┬──────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────┐
│                   Library (lib.rs)                   │
│                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │ Category  │  │   Dice   │  │  ScoreSheet      │  │
│  │ 13 enums  │  │ 5 dice   │  │  filled/unfilled │  │
│  └──────────┘  └──────────┘  └──────────────────┘  │
│                                                     │
│  ┌──────────────────────────────────────────────┐   │
│  │              GameState                        │   │
│  │  dice: Option<Dice> | rolls_used | sheet     │   │
│  └──────────────────────────────────────────────┘   │
│                                                     │
│  ┌──────────────────────────────────────────────┐   │
│  │                Rules                          │   │
│  │  score() | legal_actions() | apply_score()   │   │
│  │  per-ruleset variants for Hasbro/BBG         │   │
│  └──────────────────────────────────────────────┘   │
│                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │  Agent   │  │ Decision │  │ DecisionSource   │  │
│  │  trait   │  │  struct  │  │ oracle/table/    │  │
│  └──────────┘  └──────────┘  │ heuristic        │  │
│                              └──────────────────┘  │
│                                                     │
│  ┌──────────────────────────────────────────────┐   │
│  │         Agent Implementations                │   │
│  │  HeuristicAgent | HybridAgent               │   │
│  │  ExactTableAgent | OptimalAgent             │   │
│  └──────────────────────────────────────────────┘   │
│                                                     │
│  ┌──────────────────────────────────────────────┐   │
│  │         Table Systems                         │   │
│  │  OracleTable (TSV, endgame slices)          │   │
│  │  AnchorValueTable (binary, full game)       │   │
│  │  DenseTurnTables (internal build helper)    │   │
│  └──────────────────────────────────────────────┘   │
│                                                     │
│  ┌──────────────────────────────────────────────┐   │
│  │         GameSimulator                         │   │
│  │  deterministic PRNG (Rng64)                 │   │
│  │  plays solitaire games with agent policy    │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

## Data Flow: `advise` Command

```
CLI args: dice=1,2,3,4,6 rolls=3 scores=ones:3
        │
        ▼
GameState::parse_compact()
        │
        ▼
Rules::legal_actions(state)
        │
        ▼
Agent::select_action(state)
        │
        ├── HeuristicAgent: rule-based scoring (keep best dice, roll rest)
        ├── HybridAgent: heuristic + OptimalAgent fallback
        ├── ExactTableAgent: lookup in precomputed AnchorValueTable
        └── OptimalAgent: recursive solver with memoization
        │
        ▼
Action → stdout (e.g., "roll hold_mask=00100")
```

## Agent Tier Decision Flow

```
User selects agent
        │
        ▼
  ┌─────────────┐
  │ Heuristic   │── Fast, rule-based, ~75-85% of optimal
  │ (default for │    no precomputed data needed
  │  comparison) │
  └─────────────┘
        │
  ┌─────────────┐
  │ Hybrid      │── Heuristic for early turns,
  │              │    OptimalAgent for last 2-3 categories
  └─────────────┘
        │
  ┌──────────────────┐
  │ ExactTableAgent  │── Lookup in AnchorValueTable,
  │ (default for     │    near-optimal play
  │  production)     │
  └──────────────────┘
        │
  ┌──────────────┐
  │ OptimalAgent │── Exact recursive solver,
  │              │    memoized, on-demand computation
  └──────────────┘
```

## Table System Lifecycle

### OracleTable (TSV)

- Built with `build-table` command
- Targets endgame slices: exactly one open category
- Uses `OptimalAgent` to compute best action for every canonical dice state
- Output: human-readable TSV for review and debugging
- Use case: verify agent behavior on known endgame positions

### AnchorValueTable (Binary)

- Built with `build-anchor-table` command
- Covers the full game: all 13 categories open down to zero
- Built in layers by open category count (0 → 13)
- Each layer depends only on the previous layer (bottom-up DP)
- Uses `AnchorBuildStrategy::Dense` (precomputed transition tables) by default
- Outputs binary file with magic header `KEIRIAT1`, version 2
- Verified against opening expected value: 254.589609 (BBG ruleset)
- Use case: production-grade advisor via `ExactTableAgent`

### Build Process

```
build_anchor_table_command()
        │
        ▼
AnchorValueTable::build_limited_with_options_and_progress()
        │
        ▼
build_missing_layers_with_callbacks()
        │
        ├── For each open_count from 0 to max_open_categories:
        │   │
        │   ├── Check if layer already complete (skip)
        │   │
        │   ├── open_count == 0: fill with 0.0 (terminal anchor)
        │   │
        │   └── open_count > 0:
        │       │
        │       ├── Dense strategy:
        │       │   build_anchor_layer_dense()
        │       │   → DenseTurnTables for fast transitions
        │       │   → Thread pool with work stealing
        │       │
        │       └── Recursive strategy:
        │           build_anchor_layer_recursive()
        │           → Per-key TurnSolver instances
        │           → Thread scope for parallelism
        │
        │       → layer_done callback (save .partial checkpoint)
        │
        ▼
save() → atomic write with checksum64
```

## Key Design Decisions

### Zero Dependencies

Keiri uses only `std` (Rust edition 2024). No `serde`, no `anyhow`, no external
crates. This means:
- Binary size is tiny
- Compile time is fast
- All serialization is manual (binary table format, compact state strings)
- Error handling uses a custom `KeiriError` enum

### 252 Canonical Dice States

Five dice have 6^5 = 7,776 ordered outcomes. Sorting dice values reduces this
to 252 canonical states. The agent operates on canonical states only, giving a
29x reduction in state space. Canonical states are computed via `Dice::sort()`
and cached in lookup tables.

### Layered Anchor Tables

Building the full anchor table in one pass would require holding all intermediate
values in memory. Instead, Keiri builds layer by layer, where each layer
(open category count) depends only on the previous layer. This enables:
- Checkpointing: each completed layer is saved to a `.partial` file
- Resume: interrupted builds continue from the last completed layer
- Memory efficiency: only two layers need to be in memory at once

### Deterministic PRNG

`Rng64` is a seeded pseudo-random number generator. All simulations are
deterministic given the same seed. This makes:
- Bug reproduction reliable
- Performance comparison fair (same dice sequences)
- Table verification reproducible

### Ruleset Abstraction

Two rulesets are supported:
- `Ruleset::HasbroStrict`: Standard Hasbro Yahtzee with forced Joker behavior
- `Ruleset::BuddyBoardGames`: Free-choice Joker (Yahtzee can score any category)

The `Rules` impl methods accept a `Ruleset` parameter and branch on ruleset
for scoring, Joker activation, and legal category selection. This keeps the
core logic shared while allowing ruleset-specific behavior.
