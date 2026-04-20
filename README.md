# Keiri

Keiri is a Rust-first Yahtzee oracle MVP. It provides a rules-perfect solitaire
Yahtzee core, deterministic transition engine, legal action generation, and an
exact dynamic-programming advisor for bounded states.

The current implementation targets US Hasbro-style solitaire Yahtzee:

- 13 categories
- 35-point upper-section bonus at 63 points
- 100-point additional Yahtzee bonuses
- Joker rules for additional Yahtzees after the Yahtzee category scored 50
- Up to three rolls per turn

## Commands

```bash
cargo run -- simulate
cargo run -- --simulate seed=42 verbose=true
cargo simulate
cargo run -- --bbg-join my-room-code
cargo run -- bbg-join room=my-room-code player=keiri-bot play=true
cargo run -- evaluate games=1000 seed=1 oracle_endgame=0 out=metrics/simulation_history.csv scores_out=metrics/scores.csv
cargo run -- simulate rules=buddyboardgames agent=auto table=target/keiri_tables/bbg-anchor-v1.bin seed=42
cargo run -- score full-house 2,2,3,3,3
cargo run -- actions dice=1,2,3,4,5 rolls=2 scores=ones:3,twos:6
cargo run -- advise dice=1,2,3,4,6 rolls=3 scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50
cargo run -- bbg-advise state=STARTED me=0 turn=0 spectator=false pending=false dice=1,2,3,4,5 selected=0,0,0,0,0 rolls=2 rows=0:3:1,1:6:1
cargo run -- build-table out=target/keiri_tables/chance.tsv depths=2,3 scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50
cargo run -- build-anchor-table rules=buddyboardgames out=target/keiri_tables/bbg-anchor-v1.bin threads=auto builder=dense
```

`cargo run --simulate` cannot work because Cargo consumes `--simulate` before
Keiri starts. Use `cargo run -- --simulate`, `cargo run -- simulate`, or
`cargo simulate`.

State arguments use `key=value` tokens:

- `dice=1,2,3,4,5` or `dice=none`
- `rolls=0..3`
- `scores=category:score,category:score`
- `yahtzee_bonus=<count>`

The parser is also available as `GameState::parse_compact` and
`GameState::parse_compact_tokens` for library callers. Parsed score sheets reject
impossible recorded scores such as `twos:3`, `full-house:20`, or `yahtzee:49`.
The CLI intentionally uses only the Rust standard library. Category names accept
common aliases such as `three-kind`, `3kind`, `full-house`, and `yahtzee`.

## Oracle Tables

`build-table` writes deterministic TSV files for bounded endgame slices. The
input score sheet must have exactly one open category, which keeps the first
offline table path fast and reviewable while the full-game table design matures.
`build-anchor-table` writes the dense binary table used by exact live play. The
default builder is an exact bottom-up dense dynamic-programming pass over the
252 canonical dice states, and `threads=auto` uses all available CPU parallelism.
Use `builder=recursive` only when debugging against the older reference builder.

The output contains one row per canonical sorted dice state and requested roll
depth:

```text
state	action	expected_value
dice=1,1,1,1,1 rolls=3 ...	score chance	5.000000
```

## Simulation

`simulate` plays one solitaire game. Both Hasbro and BuddyBoardGames simulations
default to `agent=auto`, which uses the exact anchor-table policy. Runs use a
fresh seed unless `seed=<u64>` is provided.

```bash
cargo run -- simulate seed=42 verbose=true rules=hasbro oracle_endgame=2
cargo run -- simulate seed=42 verbose=true rules=buddyboardgames agent=auto table=target/keiri_tables/bbg-anchor-v1.bin
```

The default output is only the final score, which makes it easy to script:

```text
200
```

`evaluate` runs many deterministic games and records summary metrics for plotting
agent changes over time:

```bash
cargo run -- evaluate games=10000 seed=1 oracle_endgame=0 out=metrics/simulation_history.csv scores_out=metrics/scores.csv
```

The summary CSV includes timestamp, agent name, ruleset, game count, seed, mean,
min, p05, p50, p95, max, upper-bonus rate, and Yahtzee-bonus rate. `scores_out`
writes one score per game for distribution plots. Use `oracle_endgame=2` for
stronger single-game play, but keep `oracle_endgame=0` for large batches unless
you intentionally want the slower exact endgame search in every game.
`evaluate` defaults to exact `agent=auto` for both rulesets; pass
`agent=heuristic` or `agent=hybrid` only for fallback comparisons.

## BuddyBoardGames

Keiri has an opt-in BuddyBoardGames rules variant and adapter. The adapter reads
a compact snapshot of the live page state and returns a guarded site action.
The BuddyBoardGames variant uses free-choice Joker legality: a later Yahtzee may
be scored in any open category, and Full House/straight Joker scores apply after
both the Yahtzee row and the matching upper row are filled. A zeroed Yahtzee row
enables later Joker scoring but does not earn Yahtzee bonuses.
Live `bbg-advise` defaults to `agent=auto`: it uses the exact anchor-table agent
and builds plus verifies the table on first run if it is missing. Pass
`agent=heuristic` only to force the older heuristic fallback.

To join a BuddyBoardGames Yahtzee lobby directly from Keiri:

```bash
cargo run -- --bbg-join my-room-code
```

That opens a headed browser, enters the room code, joins as `keiri-bot`, and
plays whenever it is Keiri's turn. If the exact table has not been built yet,
Keiri builds it with the optimized release binary, prints layer progress, saves
layer checkpoints to `<table>.partial`, saves the final table atomically,
reloads it, and verifies it before opening the browser. A full BuddyBoardGames
table build on an M4 Pro is expected to complete in seconds rather than hours,
and the verified opening expected value is `254.589609`. Interrupted builds
resume from the latest compatible completed layer. If the room did not exist and
Keiri is alone in the lobby, the helper starts the game automatically and plays
the solo game to completion. Leave the process running; stop it with Ctrl-C. If
you omit the room code, Keiri prompts for it:

```bash
cargo run -- --bbg-join
```

Options:

```bash
cargo run -- bbg-join room=my-room-code player=keiri-bot play=true start=false
cargo run -- bbg-join room=my-room-code player=keiri-bot play=false
```

For browser use, run the Playwright helper in dry-run mode first:

```bash
npx --yes --package playwright node tools/buddyboardgames/autoplay.mjs --dry-run --url=https://www.buddyboardgames.com/yahtzee
```

Use `--execute` only in your own room when you want it to click the advised
hold/roll/score action.

The Playwright helper writes execution traces to `target/bbg-traces/` after
clicking. Each JSONL event includes the page snapshot, exact advice output, and
post-click state so low-score games can be replayed and audited.

## Library API

The public API centers on:

- `Category`
- `Dice`
- `ScoreSheet`
- `GameState`
- `Action`
- `Rules`
- `Ruleset`
- `Agent`
- `HybridAgent`
- `ExactTableAgent`
- `OptimalAgent`
- `AnchorValueTable`
- `OracleTable`
- `GameSimulator`
- `BuddyBoardGamesSnapshot`

`OptimalAgent` owns its memoization cache, so one instance can be reused across
many related queries.

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
