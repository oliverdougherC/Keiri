# Oracle

Keiri's `OptimalAgent` is an exact expected-value solver for solitaire Yahtzee
states. It recursively evaluates legal score and hold actions, then memoizes
canonical states.

## State Canonicalization

The oracle key contains only future-relevant information:

- Current sorted dice, if a turn is in progress
- Rolls used in the current turn
- Filled category mask
- Upper subtotal capped at 63
- Whether the Yahtzee category has scored 50

Past lower-section scores are not part of the key because they do not affect
future legal moves or future expected points. Existing Yahtzee bonus count is
also excluded because the oracle value is the expected additional score from the
current state onward.

## Reroll Distributions

Rerolls are enumerated as compressed sorted outcomes with multiplicity weights.
For example, rerolling five dice uses 252 sorted outcomes whose weights sum to
`6^5`, rather than iterating all 7,776 ordered rolls.

## Performance Notes

The recursive solver is correctness-first and computes values on demand. Endgame
and midgame states are practical; asking it for an opening-game value can still
explore a very large state space. Production-scale live play should use the
dense anchor table and `ExactTableAgent`.

`OptimalAgent` owns its cache. Reuse the same agent for related queries to avoid
recomputing shared subtrees.

## Offline Table Slices

`OracleTable::build_endgame` and the `keiri build-table` CLI command create
bounded TSV tables for score sheets with exactly one open category. Each table
enumerates all 252 canonical sorted dice states for the requested roll depths,
records the best action, and records the expected value from that state.

Example:

```bash
cargo run -- build-table \
  out=target/keiri_tables/chance.tsv \
  depths=2,3 \
  scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50
```

## Exact Anchor Tables

`AnchorValueTable` stores start-of-turn values keyed by filled category mask,
upper subtotal capped at 63, ruleset, and Yahtzee box state. The table builder
uses an exact dense dynamic-programming solver over the 252 canonical dice states
and three in-turn roll depths. `TurnSolver` remains the recursive reference and
uses the completed table to solve live in-turn states exactly, including every
legal score and every canonical hold choice.

```bash
cargo run -- build-anchor-table \
  rules=buddyboardgames \
  out=target/keiri_tables/bbg-anchor-v1.bin \
  threads=auto \
  builder=dense
```

BuddyBoardGames live advice defaults to this exact table. If the table is
missing or stale, `agent=auto` builds and verifies the dense table instead of
falling back to the heuristic policy. Use `builder=recursive` only for reference
debugging.
