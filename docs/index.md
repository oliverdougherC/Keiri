# Documentation Index

Welcome to the Keiri documentation. Start here for an overview, then dive into
the topic that matches your needs.

## Getting Started

- [Architecture Overview](architecture.md) — System design, component diagram,
  data flow, agent tiers, and key design decisions. Read this first to
  understand how Keiri works at a high level.
- [CLI Reference](cli-reference.md) — All 10 commands with usage, flags, and
  examples. Use this as a quick lookup when running Keiri from the terminal.

## Deep Dives

- [Agent System](agents.md) — The `Agent` trait, HeuristicAgent, HybridAgent,
  ExactTableAgent, and OptimalAgent. How each works, when to use it, and how
  to select one from the CLI.
- [State Format](state-format.md) — Canonical dice states (252), oracle keys,
  GameState encoding, action representation, and binary table format.
- [Public API](api-reference.md) — Library usage, public types (Category,
  Dice, ScoreSheet, GameState, Agent, AnchorValueTable, etc.), and common
  operations.

## Rules and Simulation

- [Ruleset Reference](ruleset.md) — Hasbro Yahtzee rules, 13 categories,
  scoring logic, upper-section bonus, Yahtzee bonuses, and Joker rules.
- [Oracle Tables](oracle.md) — OptimalAgent recursive solver, memoization,
  reroll distributions, offline TSV tables, and anchor value tables.

## BuddyBoardGames Integration

- [BuddyBoardGames](buddyboardgames.md) — Rules variant, CLI snapshot format,
  Playwright helper usage, anchor table building, and solo grinding.

## Project Conventions

- [CLAUDE.md](../CLAUDE.md) — Project conventions for AI assistants and
  contributors. Rust style, module organization, how to add CLI commands,
  agents, and rulesets.
