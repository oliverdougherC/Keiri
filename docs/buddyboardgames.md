# BuddyBoardGames Integration

Keiri includes a first BuddyBoardGames integration path for Yahtzee.

The integration is intentionally guarded:

- It only acts when the page reports `STARTED`.
- It only acts when `meIdx == turnIdx`.
- It refuses spectator mode.
- It refuses pending roll/update states.
- It reads score rows and dice from the page before every action.

## Rules Variant

BuddyBoardGames uses the free-choice Joker behavior used by the optimal
solitaire benchmark. A Yahtzee roll can still be assigned to any open category.
Full House, Small Straight, and Large Straight receive their fixed Joker scores
only after both the Yahtzee row and the matching upper row have already been
filled. If the Yahtzee row was scored as zero, a later Yahtzee can still be used
as a Joker, but it does not earn a Yahtzee bonus.

Keiri keeps the older forced-upper behavior under `Ruleset::HasbroStrict` and
exposes the BuddyBoardGames behavior as `Ruleset::BuddyBoardGames`.

## CLI Snapshot

`bbg-advise` accepts a compact snapshot:

```bash
cargo run -- bbg-advise \
  state=STARTED \
  me=0 \
  turn=0 \
  spectator=false \
  pending=false \
  dice=1,2,3,4,5 \
  selected=0,0,0,0,0 \
  rolls=2 \
  rows=0:3:1,1:6:1
```

Rows use BuddyBoardGames client row indexes:

- `0..5`: ones through sixes
- `6`: bonus, not selectable
- `7..13`: three-kind through chance
- total rows are not selectable

## Playwright Helper

`bbg-advise` and the Playwright helper default to `agent=auto`, which uses the
exact table-backed agent and builds plus verifies the table on first run if it
is missing. The build uses the optimized release binary and exact dense
dynamic programming over canonical dice states, prints layer progress,
checkpoints completed layers to `<table>.partial`, writes the final table
atomically, reloads, and verifies the result. Interrupted builds resume from the
last compatible completed layer. To build it explicitly ahead of time:

```bash
cargo run -- build-anchor-table \
  rules=buddyboardgames \
  out=target/keiri_tables/bbg-anchor-v1.bin \
  threads=auto \
  builder=dense
```

Use `agent=heuristic` only when intentionally falling back to the older
heuristic path.

Join a lobby through Keiri:

```bash
cargo run -- --bbg-join my-room-code
```

This launches the Playwright helper, keeps the browser connected, and plays
whenever it is Keiri's turn. Use `player=<name>` to override the default bot
name. When the requested room does not exist, BuddyBoardGames creates a lobby;
if Keiri is the only player there, the helper starts it automatically and plays
the solo game to completion:

```bash
cargo run -- bbg-join room=my-room-code player=Keiri play=true
```

To only join and wait without playing:

```bash
cargo run -- bbg-join room=my-room-code player=Keiri play=false
```

To keep rematching and replaying solo games in the same room:

```bash
cargo run -- bbg-loop room=my-room-code player=Keiri
```

Stop the loop with Ctrl-C or `SIGTERM` to get a final session summary. The
helper now requests a graceful stop first, then reports the number of completed
games, the highest score seen, and the mean score. It writes a PNG
score-history chart with a dashed mean line to `target/bbg-reports/`. If
`matplotlib` is missing, the helper first reuses an existing virtual
environment when present; otherwise it creates a repo-local `.venv` with `uv`
and installs `matplotlib` there. When `uv` is unavailable, it falls back to
`pip`.

Dry run:

```bash
npx --yes --package playwright node tools/buddyboardgames/autoplay.mjs --dry-run --url=https://www.buddyboardgames.com/yahtzee
```

The default landing page is often in `DEMO` or `LOBBY`, so dry-run now reports a
guarded waiting status instead of failing. To force a brand-new solo room into a
live game before asking for advice, add `--player`, `--room`, and `--start-game`.

Join a room and inspect the recommendation:

```bash
npx --yes --package playwright node tools/buddyboardgames/autoplay.mjs \
  --dry-run \
  --player=Keiri \
  --room=my-room \
  --start-game
```

Execute one guarded action:

```bash
npx --yes --package playwright node tools/buddyboardgames/autoplay.mjs \
  --execute \
  --player=Keiri \
  --room=my-room
```

The helper launches a browser, reads `thisGame`, calls `cargo run -- bbg-advise`,
and clicks only the returned dice/roll/score selector.
Executed actions are appended to `target/bbg-traces/*.jsonl` with the raw
snapshot, exact advice output, parsed action, and post-click state.
