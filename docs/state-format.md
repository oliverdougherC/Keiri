# State Format

This document describes how game states are encoded in Keiri, from raw dice
values through canonical representations to binary table storage.

## Canonical Dice States

Five dice have 6^5 = 7,776 ordered outcomes. Sorting dice values reduces this
to 252 canonical states. Keiri operates on canonical states exclusively.

### Canonical State Encoding

A canonical state is represented as a sorted sequence of 5 dice values
(1-6), where dice are sorted in ascending order. For example:

- Unsorted roll: `[6, 1, 3, 2, 5]` → Sorted: `[1, 2, 3, 5, 6]`
- Unsorted roll: `[4, 4, 2, 4, 1]` → Sorted: `[1, 2, 4, 4, 4]`

The `Dice::sort()` method computes the sorted order and caches it. Canonical
states are keyed by a compact representation used in hash maps and table lookups.

### Why 252?

The number 252 comes from combinations with repetition: C(n+k-1, k) where
n=6 faces and k=5 dice = C(10, 5) = 252. This is the number of distinct
multisets of 5 dice values from a 6-sided die.

### Canonical State Lookup Tables

Keiri precomputes transition tables that map (canonical_state, hold_mask) pairs
to distributions of next-state canonical outcomes. These tables are the core of
the `DenseTurnTables` system used by the anchor table builder.

## Oracle Key

The oracle solver uses an `AnchorKey` to identify game states. An anchor key
encodes:

- **Open categories**: A bitmask of which of the 13 categories are still open
- **Open count**: The number of open categories (derived from bitmask)
- **Upper scores**: Sum of scores in the 6 upper categories (ones through sixes)
- **Yahtzee state**: Current `AnchorYahtzeeState` (none, yahtzee_scored, yahtzee_joker_eligible)
- **Yahtzee count**: Number of Yahtzees scored (for joker eligibility)

The `AnchorKey` is the memoization key for `OptimalAgent` and the lookup key
for `AnchorValueTable` layers.

## GameState Encoding

`GameState` is the runtime representation of a solitaire Yahtzee turn. It
contains:

- `dice`: `Option<Dice>` — current dice values (None means not yet rolled)
- `rolls_used`: u8 — number of rolls taken (0-2, since 3 rolls max)
- `sheet`: ScoreSheet — current score sheet state
- `yahtzee_bonuses`: u32 — count of Yahtzee bonuses accrued

### Compact State String

Game states can be encoded as compact strings for CLI and serialization:

```
dice=1,2,3,4,5 rolls=2 scores=ones:3,twos:6
```

- `dice=none` means dice not yet rolled
- `rolls=0..3` — number of rolls already used
- `scores=category:score,category:score` — filled categories with their scores
- `yahtzee_bonus=<count>` — optional Yahtzee bonus count

The parser is available as `GameState::parse_compact` and
`GameState::parse_compact_tokens` for library callers.

### Validation

Parsed score sheets reject impossible recorded scores:
- `twos:3` — can't score 3 in twos (must be 0, 2, 4, 6, 8, 10, or 12)
- `full-house:20` — max Full House score is 25 (5x5)
- `yahtzee:49` — Yahtzee is always exactly 50 (or 0 via Joker)

## Action Encoding

Actions represent what a player can do in a given state:

- **Roll**: `roll hold_mask=<5-bit mask>` — keep dice where mask bit is 1, reroll where 0
  - Hold mask is a 5-bit integer (0-31), one bit per die position
  - Example: `hold_mask=00100` means keep the 3rd die, reroll the rest
- **Score**: `score category=<category_name>` — assign current dice to a category
  - Category names match the 13 Yahtzee categories

### Legal Actions

`Rules::legal_actions_with_ruleset(state, ruleset)` returns the set of legal
actions for a given state. This includes:
- Roll actions (if rolls_used < 3)
- Score actions (if dice are present and category is open)

## Binary Table Format

The `AnchorValueTable` uses a binary format for fast loading and compact storage.

### File Header

```
Magic:     "KEIRIAT1" (8 bytes)
Version:   u32 (currently 2)
Ruleset:   u8 (0=HasbroStrict, 1=BuddyBoardGames)
OpenCount: u8 (max open categories covered)
LayerCount: u32 (number of layers stored)
Checksum:  checksum64 (over file contents, excluding checksum field)
```

### Layer Structure

Each layer stores values for a specific open category count (0 to 13). Layers
are stored sequentially:

```
For each open_count:
  For each canonical dice state (252 states):
    For each AnchorKey with that open_count:
      expected_value: f64 (8 bytes)
```

### Checksum

A `checksum64` is computed over the file contents (excluding the checksum field
itself) to detect corruption. The table verifies its checksum on load.

### Atomic Write

Tables are written atomically to prevent corruption from interrupted writes:
1. Write to a temporary `.tmp` file
2. Compute and append checksum
3. Rename to final path (atomic on POSIX)

### Partial Checkpoints

During table building, each completed layer is saved to a `<table>.partial`
file. Interrupted builds resume from the last completed layer by loading the
partial file and continuing from the next incomplete layer.

## TSV Table Format

The `OracleTable` (built via `build-table`) uses a human-readable TSV format:

```
state	action	expected_value
dice=1,1,1,1,1 rolls=3	score chance	5.000000
```

- `state`: Canonical dice state string (sorted dice + roll depth)
- `action`: Best action string (score category or roll with hold mask)
- `expected_value`: Float64 expected value of the action

This format is intended for human review and debugging, not production use.
