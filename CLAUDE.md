# CLAUDE.md — Keiri Project Conventions

## Overview

Keiri is a zero-dependency Rust Yahtzee oracle engine (edition 2024, std only).
It provides rules-perfect solitaire Yahtzee simulation, deterministic transition
engine, legal action generation, and an exact dynamic-programming advisor for
bounded states.

## Rust Conventions

- Edition 2024, `extern crate std` only, no external dependencies
- `snake_case` functions and variables, `PascalCase` types and traits
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` on all value types
- `fmt::Display` for all types with a natural textual representation
- Error handling via `KeiriError` enum — no `unwrap()` in production code
- Constants use `SCREAMING_SNAKE_CASE`
- `pub` types are stable; internal structs/functions are `pub(crate)` or private
- Zero panics in public API — use `Result` for all fallible operations

## Module Organization

- `src/lib.rs` — All public types, traits, and functions
- `src/main.rs` — CLI commands and argument parsing (reuses lib types)
- `tests/keiri.rs` — Integration tests covering public API
- `benches/oracle.rs` — Benchmark harness for agent performance
- `docs/` — Markdown documentation

## Adding CLI Commands

1. Add a new branch to the `match` in `run()` in `src/main.rs`
2. Create a `fn command_name(args: &[String]) -> Result<(), KeiriError>`
3. Parse `key=value` tokens from args
4. Call library functions from `src/lib.rs`
5. Print results to stdout; `KeiriError` messages go to stderr via `main()`
6. Add usage text to `print_usage()`

## Adding Agents

1. Implement the `Agent` trait in `src/lib.rs`
2. Add variant to `CliAgent` enum in `src/main.rs` for CLI parsing
3. Update `parse_agent()` to handle the new variant name
4. If the agent needs precomputed data, add build logic to `build_anchor_table_command`

## Adding Rulesets

1. Add variant to `Ruleset` enum in `src/lib.rs`
2. Update `Ruleset::from_name()` and `Ruleset::name()`
3. Add ruleset-specific behavior in `Rules` impl methods:
   - `joker_active_with_ruleset()`
   - `legal_score_categories_with_ruleset()`
   - `score_with_ruleset()`
4. Update `AnchorValueTable` to track ruleset per table
5. Add tests in `tests/keiri.rs` for ruleset-specific scoring

## Build and Test Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo bench
```

## Table Building Notes

- Anchor tables are built in layers (open category count), from 0 upward
- Default builder uses `AnchorBuildStrategy::Dense` (precomputed transition tables)
- `threads=auto` uses `std::thread::available_parallelism()`
- Interrupted builds resume from last compatible completed layer via `.partial` files
- Binary format: 8-byte magic `KEIRIAT1`, version 2, ruleset byte, category order, value count, checksum64, then 8-byte f64 values
- Opening expected value for BuddyBoardGames ruleset: 254.589609
- Opening expected value for HasbroStrict ruleset: ~238.3 (varies slightly)

## Key Constants

- 252 canonical dice states (sorted dice outcomes)
- 13 categories (6 upper + 7 lower)
- 3 rolls per turn
- 3 dice per roll
- Upper section bonus threshold: 63 points → 35 point bonus
- Yahtzee bonus: 100 points per additional Yahtzee after Yahtzee category scored 50

## Existing Documentation

See `docs/` for structured documentation:
- `docs/index.md` — Documentation navigation
- `docs/architecture.md` — System design overview
- `docs/agents.md` — Agent system documentation
- `docs/state-format.md` — State encoding reference
- `docs/cli-reference.md` — Command reference
- `docs/api-reference.md` — Public API documentation
- `docs/oracle.md` — OptimalAgent and oracle tables
- `docs/ruleset.md` — Rules and categories
- `docs/buddyboardgames.md` — BuddyBoardGames integration
