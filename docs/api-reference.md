# Public API Reference

Keiri's library API is defined in `src/lib.rs`. The crate uses only `std` —
no external dependencies.

## Error Types

### KeiriError

```rust
pub enum KeiriError {
    InvalidDie(u8),
    InvalidDiceCount(usize),
    InvalidHoldMask(u8),
    InvalidRollCount(u8),
    MissingDice,
    DiceAlreadyRolled,
    NoRollsRemaining,
    CategoryAlreadyFilled(Category),
    CategoryNotLegal(Category),
    InvalidRecordedScore { category: Category, score: u16 },
    InvalidBuddyBoardGamesSnapshot(String),
    InvalidTableSlice(String),
    InvalidAnchorTable(String),
    MissingAnchorValue(String),
    TerminalState,
    ParseError(String),
}
```

All library functions return `Result<T, KeiriError>`. The error enum implements
`std::error::Error` and `Display`.

## Constants

```rust
pub const DICE_COUNT: usize = 5;
pub const DICE_STATE_COUNT: usize = 252;
pub const YAHTZEE_BONUS: u16 = 100;
pub const UPPER_BONUS: u16 = 35;
pub const UPPER_BONUS_THRESHOLD: u16 = 63;
```

## Category

```rust
pub enum Category {
    Ones, Twos, Threes, Fours, Fives, Sixes,
    ThreeKind, FourKind, FullHouse, SmallStraight,
    LargeStraight, Yahtzee, Chance,
}
```

13 Yahtzee categories. Provides:

- `Category::ALL` — array of all 13 categories
- `Category::UPPER` — array of 6 upper categories
- `Category::LOWER` — array of 7 lower categories
- `Category::from_name(name: &str) -> Option<Self>` — parse from string (accepts aliases)
- `Category::from_index(index: usize) -> Option<Self>` — parse from 0-based index
- `Category::index(self) -> usize` — 0-based index
- `Category::is_upper(self) -> bool` — true for upper section categories
- `Category::upper_face(self) -> Option<u8>` — face value for upper categories (1-6)
- `Category::upper_for_face(face: u8) -> Option<Self>` — inverse of `upper_face`

## Dice

```rust
pub struct Dice {
    values: [u8; 5], // sorted ascending
}
```

Five sorted dice values (1-6). Always stored in sorted order.

- `Dice::new(values: [u8; 5]) -> Result<Self, KeiriError>` — create with validation
- `Dice::from_slice(values: &[u8]) -> Result<Self, KeiriError>` — create from slice
- `Dice::parse(input: &str) -> Result<Self, KeiriError>` — parse comma-separated dice
- `Dice::values(self) -> [u8; 5]` — get sorted values
- `Dice::all_canonical() -> Vec<Self>` — all 252 canonical dice states

## ScoreSheet

Represents the filled portion of a Yahtzee score sheet.

- `ScoreSheet::new() -> Self` — empty score sheet (all categories open)
- `ScoreSheet::fill(category: Category, score: u16) -> Result<Self, KeiriError>` — fill a category
- `ScoreSheet::is_filled(category: Category) -> bool` — check if category is filled
- `ScoreSheet::score(category: Category) -> Option<u16>` — get recorded score
- `ScoreSheet::total_score(&self) -> u16` — sum of all filled category scores
- `ScoreSheet::upper_total(&self) -> u16` — sum of upper section scores
- `ScoreSheet::upper_bonus_eligible(&self) -> bool` — true if upper total >= 63
- `ScoreSheet::upper_bonus(&self) -> u16` — 35 if eligible, 0 otherwise
- `ScoreSheet::yahtzee_bonuses(&self) -> u32` — count of Yahtzee bonus points

## GameState

Runtime representation of a solitaire Yahtzee turn.

```rust
pub struct GameState {
    dice: Option<Dice>,
    rolls_used: u8,
    sheet: ScoreSheet,
    yahtzee_bonuses: u32,
}
```

- `GameState::new(sheet: ScoreSheet) -> Self` — start a new turn with no dice
- `GameState::roll(&self) -> Result<Dice, KeiriError>` — roll dice (increments rolls_used)
- `GameState::can_roll(&self) -> bool` — true if rolls_used < 3
- `GameState::has_dice(&self) -> bool` — true if dice are present
- `GameState::parse_compact(input: &str) -> Result<Self, KeiriError>` — parse from compact string
- `GameState::parse_compact_tokens(tokens: &[&str]) -> Result<Self, KeiriError>` — parse from key=value tokens

### Compact String Format

```
dice=1,2,3,4,5 rolls=2 scores=ones:3,twos:6
```

- `dice=none` for no dice yet
- `rolls=0..3` for rolls used
- `scores=cat:score,...` for filled categories

## Rules

```rust
pub struct Rules;
```

Static methods for scoring and legality checking.

- `Rules::score_with_ruleset(dice: Dice, category: Category, ruleset: Ruleset) -> u16`
- `Rules::legal_actions_with_ruleset(state: &GameState, ruleset: Ruleset) -> Vec<Action>`
- `Rules::legal_score_categories_with_ruleset(state: &GameState, ruleset: Ruleset) -> Vec<Category>`

## Ruleset

```rust
pub enum Ruleset {
    HasbroStrict,    // Standard Hasbro with forced Joker
    BuddyBoardGames, // Free-choice Joker
}
```

- `Ruleset::from_name(name: &str) -> Option<Self>` — parse from string
  - `"hasbro"`, `"hasbrostrict"`, `"strict"` → `HasbroStrict`
  - `"buddyboardgames"`, `"bbg"`, `"buddy"` → `BuddyBoardGames`

## Agent Trait

```rust
pub trait Agent {
    fn select_action(&self, state: &GameState, ruleset: Ruleset) -> Decision;
}
```

All agent implementations provide this trait. See [agents.md](agents.md) for
details on each implementation.

## Decision

```rust
pub struct Decision {
    pub action: Action,
    pub expected_value: Option<f64>,
    pub source: DecisionSource,
}
```

- `action`: The recommended `Action`
- `expected_value`: Optional f64 estimate
- `source`: `DecisionSource` indicating origin

## DecisionSource

```rust
pub enum DecisionSource {
    Oracle,    // OptimalAgent recursive solver
    Table,     // ExactTableAgent via AnchorValueTable
    Heuristic, // HeuristicAgent rule-based
    Hybrid,    // HybridAgent
}
```

## Action

Represents a player action (roll or score). See [state-format.md](state-format.md)
for encoding details.

## Agent Implementations

- `HeuristicAgent` — rule-based, no setup cost. See [agents.md](agents.md)
- `HybridAgent` — heuristic + optimal for late turns. See [agents.md](agents.md)
- `ExactTableAgent` — table lookup. See [agents.md](agents.md)
- `OptimalAgent` — exact recursive solver with memoization. See [agents.md](agents.md)

## AnchorValueTable

Precomputed optimal values for all game states.

- `AnchorValueTable::build_limited_with_options_and_progress(options: AnchorBuildOptions) -> Self`
- `AnchorValueTable::load(path: &str) -> Result<Self, KeiriError>` — load from binary file
- `AnchorValueTable::verify(&self) -> Result<(), KeiriError>` — verify opening expected value
- `AnchorValueTable::lookup(&self, key: &AnchorKey, canonical_dice: Dice) -> Option<f64>` — get stored value

### AnchorKey

Memoization key for game states. Encodes open categories, open count, upper
scores, and Yahtzee state. See [state-format.md](state-format.md).

### AnchorYahtzeeState

Tracks Yahtzee category status for joker eligibility:

- `None` — Yahtzee not yet scored
- `YahtzeeScored` — Yahtzee category filled with 50
- `YahtzeeJokerEligible` — Yahtzee scored but zeroed (can still use as joker)

### AnchorBuildStrategy

```rust
pub enum AnchorBuildStrategy {
    Dense,       // Precomputed transition tables (default)
    Recursive,   // Per-key TurnSolver instances
}
```

### AnchorBuildOptions

```rust
pub struct AnchorBuildOptions {
    pub ruleset: Ruleset,
    pub max_open_categories: usize,
    pub builder: AnchorBuildStrategy,
    pub threads: usize,
    pub on_layer_done: Option<Box<dyn Fn(AnchorBuildProgress) + Send>>,
}
```

## OracleTable

Endgame TSV table for single-open-category slices.

- `OracleTable::build(state: &GameState, depth: usize, agent: &dyn Agent) -> Self`
- `OracleTable::write_tsv(&self, path: &str)` — write TSV file
- `OracleTable::lookup(&self, canonical_dice: Dice, rolls_remaining: u8) -> Option<f64>`

## GameSimulator

Deterministic solitaire game simulator.

- `GameSimulator::new(seed: u64) -> Self`
- `GameSimulator::play(agent: &dyn Agent, ruleset: Ruleset) -> u16` — play one game
- `GameSimulator::play_verbose(agent: &dyn Agent, ruleset: Ruleset) -> GameRecord` — play with detailed output

## BuddyBoardGamesSnapshot

Snapshot of BuddyBoardGames page state for `bbg-advise`.

- `BuddyBoardGamesSnapshot::parse(input: &str) -> Result<Self, KeiriError>` — parse snapshot string
- Fields: `state`, `me_idx`, `turn_idx`, `spectator`, `pending`, `dice`, `selected`, `rolls`, `rows`

## Rng64

Seeded pseudo-random number generator for deterministic simulations.

- `Rng64::new(seed: u64) -> Self`
- `Rng64::next_u64(&mut self) -> u64`
- `Rng64::next_dice(&mut self) -> [u8; 5]` — generate 5 random dice values (1-6)
