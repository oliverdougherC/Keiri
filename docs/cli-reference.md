# CLI Reference

All Keiri commands use `key=value` argument tokens. The general form is:

```bash
cargo run -- <command> [key=value ...]
```

Or via Cargo alias:

```bash
cargo <command> [key=value ...]
```

## Common Arguments

These arguments work across multiple commands:

| Argument | Description | Example |
|----------|-------------|---------|
| `seed` | PRNG seed for deterministic simulation | `seed=42` |
| `rules` | Ruleset: `hasbro` or `buddyboardgames` | `rules=buddyboardgames` |
| `agent` | Agent selection: `heuristic`, `hybrid`, `auto`, `optimal` | `agent=auto` |
| `table` | Path to anchor table file | `table=target/keiri_tables/bbg-anchor-v1.bin` |
| `threads` | Thread count for table building (`auto` for all cores) | `threads=auto` |
| `builder` | Table builder: `dense` or `recursive` | `builder=dense` |
| `verbose` | Print detailed game output | `verbose=true` |

## State Arguments

State arguments use `key=value` tokens:

| Argument | Description | Example |
|----------|-------------|---------|
| `dice` | Dice values (1-6) or `none` | `dice=1,2,3,4,5` or `dice=none` |
| `rolls` | Rolls used (0-2) | `rolls=2` |
| `scores` | Filled categories | `scores=ones:3,twos:6` |
| `yahtzee_bonus` | Yahtzee bonus count | `yahtzee_bonus=2` |

Category names accept common aliases: `three-kind`, `3kind`, `full-house`,
`yahtzee`, etc.

---

## Command Reference

### simulate

Plays one solitaire Yahtzee game with the selected agent.

```bash
cargo run -- simulate [seed=N] [rules=hasbro\|buddyboardgames] [agent=heuristic\|hybrid\|auto\|optimal] [table=PATH] [verbose=true\|false]
```

**Output:** Final score (single integer to stdout).

**Examples:**

```bash
# Play one game with default agent and rules
cargo run -- simulate

# Play with specific seed for reproducibility
cargo run -- --simulate seed=42 verbose=true

# Play with BBG rules and exact table agent
cargo run -- simulate rules=buddyboardgames agent=auto table=target/keiri_tables/bbg-anchor-v1.bin seed=42

# Play with heuristic agent for comparison
cargo run -- simulate agent=heuristic seed=42
```

### bbg-join

Join a BuddyBoardGames Yahtzee lobby and play.

```bash
cargo run -- --bbg-join [room=CODE] [player=NAME] [play=true\|false]
```

**Behavior:**
- Opens a headed browser and joins the specified room
- Plays whenever it is Keiri's turn
- If room doesn't exist and Keiri is alone, starts solo game automatically
- Builds anchor table on first run if missing

**Examples:**

```bash
# Join and play (prompts for room code if omitted)
cargo run -- --bbg-join

# Join specific room with custom name
cargo run -- bbg-join room=my-room-code player=Keiri play=true

# Join but don't auto-start game
cargo run -- bbg-join room=my-room-code player=Keiri play=false
```

### bbg-loop

Keep rematching in the same BuddyBoardGames room for solo grinding.

```bash
cargo run -- bbg-loop room=CODE [player=NAME]
```

**Output:** Session summary on Ctrl-C/SIGTERM (games played, highest score, mean score).
Also writes a PNG score-history chart to `target/bbg-reports/`.

**Examples:**

```bash
# Grind solo games indefinitely
cargo run -- bbg-loop room=my-room-code player=Keiri

# Stop with Ctrl-C to see session summary
```

### evaluate

Runs many deterministic simulation games and outputs summary metrics.

```bash
cargo run -- evaluate games=N [seed=SEED] [oracle_endgame=N] [out=PATH] [scores_out=PATH] [agent=...] [rules=...]
```

**Output:** Summary CSV with timestamp, agent name, ruleset, game count, mean,
min, p05, p50, p95, max, upper-bonus rate, Yahtzee-bonus rate.

**Examples:**

```bash
# Run 1000 games, output to CSV
cargo run -- evaluate games=1000 seed=1 out=metrics/simulation_history.csv scores_out=metrics/scores.csv

# Use exact endgame search for stronger play
cargo run -- evaluate games=1000 seed=1 oracle_endgame=2 out=metrics/history.csv
```

> **Note:** Keep `oracle_endgame=0` for large batches unless you want the slower
> exact endgame search in every game.

### score

Computes the score for a specific category and dice roll.

```bash
cargo run -- score <category> dice=d1,d2,d3,d4,d5
```

**Output:** Integer score.

**Examples:**

```bash
cargo run -- score full-house 2,2,3,3,3
# Output: 25

cargo run -- score ones 1,2,3,4,5
# Output: 1
```

### actions

Lists all legal actions for a given game state.

```bash
cargo run -- actions [dice=d1,d2,d3,d4,d5] [rolls=N] [scores=cat:score,...]
```

**Output:** List of legal actions (roll with hold mask, or score in open category).

**Examples:**

```bash
# All legal actions with 2 rolls used, some categories filled
cargo run -- actions dice=1,2,3,4,5 rolls=2 scores=ones:3,twos:6
```

### advise

Gets the recommended action for a given game state.

```bash
cargo run -- advise [agent=heuristic\|hybrid\|auto\|optimal] [dice=d1,d2,d3,d4,d5] [rolls=N] [scores=cat:score,...]
```

**Output:** Recommended action string (e.g., `roll hold_mask=00100` or `score yahtzee`).

**Examples:**

```bash
# Get advice with exact table agent (default)
cargo run -- advise dice=1,2,3,4,6 rolls=3 scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50

# Get advice with heuristic agent
cargo run -- advise agent=heuristic dice=1,2,3,4,6 rolls=3
```

### bbg-advise

Gets advice from a BuddyBoardGames page snapshot.

```bash
cargo run -- bbg-advise state=STATE me=N turn=N spectator=true\|false pending=true\|false [dice=d1,d2,d3,d4,d5] [selected=0/1,...] [rolls=N] [rows=idx:score:filled,...]
```

**Snapshot fields:**
- `state`: Page state (`STARTED`, `LOBBY`, `DEMO`, etc.)
- `me`: Player index (0-based)
- `turn`: Current turn index (whose turn it is)
- `spectator`: Whether Keiri is spectating
- `pending`: Whether a roll/update is pending
- `dice`: Current dice values
- `selected`: Which dice are held (0=not held, 1=held)
- `rolls`: Rolls used
- `rows`: Row data in format `row_index:score:filled`

**Row indexes:**
- `0..5`: ones through sixes (upper section)
- `6`: bonus row (not selectable)
- `7..13`: three-kind through chance (lower section)

**Examples:**

```bash
cargo run -- bbg-advise \
  state=STARTED me=0 turn=0 spectator=false pending=false \
  dice=1,2,3,4,5 selected=0,0,0,0,0 rolls=2 \
  rows=0:3:1,1:6:1
```

### build-table

Builds an OracleTable (TSV) for endgame slices with exactly one open category.

```bash
cargo run -- build-table out=PATH [depth=N] [scores=cat:score,...]
```

**Output:** TSV file with one row per canonical dice state and roll depth.

**Examples:**

```bash
# Build endgame table for a specific score sheet state
cargo run -- build-table out=target/keiri_tables/chance.tsv depths=3 \
  scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50
```

### build-anchor-table

Builds an AnchorValueTable (binary) for the full game.

```bash
cargo run -- build-anchor-table rules=hasbro\|buddyboardgames out=PATH [threads=auto\|N] [builder=dense\|recursive]
```

**Output:** Binary file with magic header `KEIRIAT1`, version 2.

**Behavior:**
- Builds in layers (0 to 13 open categories)
- Checkpoints each layer to `<table>.partial`
- Resumes from last completed layer on re-run
- Verifies opening expected value after build (BBG: 254.589609)

**Examples:**

```bash
# Build BBG anchor table with all CPU cores
cargo run -- build-anchor-table rules=buddyboardgames out=target/keiri_tables/bbg-anchor-v1.bin threads=auto builder=dense
```
