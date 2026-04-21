use std::process::Command;

use keiri::{
    Action, AnchorBuildOptions, AnchorBuildStrategy, AnchorValueTable, BuddyBoardGamesSnapshot,
    Category, DecisionSource, Dice, GameSimulator, GameState, HybridAgent, OptimalAgent,
    OracleTable, Rules, Ruleset, ScoreSheet, advise_buddyboardgames_snapshot,
    advise_buddyboardgames_snapshot_exact, bbg_category_to_client_row, bbg_client_row_to_category,
};

const BBG_FULL_TABLE_PATH: &str = "target/keiri_tables/bbg-anchor-v1.bin";
const BBG_OPENING_EXPECTED_VALUE: f64 = 254.5896;
const BBG_OPENING_TOLERANCE: f64 = 0.001;

fn dice(values: [u8; 5]) -> Dice {
    Dice::new(values).unwrap()
}

fn sheet_filled_except(open: Category) -> ScoreSheet {
    let mut sheet = ScoreSheet::new();
    for category in Category::ALL {
        if category != open {
            let score = if category == Category::Yahtzee { 50 } else { 0 };
            sheet.fill_raw(category, score).unwrap();
        }
    }
    sheet
}

#[test]
fn upper_bonus_is_awarded_when_threshold_is_crossed() {
    let mut sheet = ScoreSheet::new();
    sheet.fill_raw(Category::Ones, 3).unwrap();
    sheet.fill_raw(Category::Twos, 6).unwrap();
    sheet.fill_raw(Category::Threes, 9).unwrap();
    sheet.fill_raw(Category::Fours, 12).unwrap();
    sheet.fill_raw(Category::Fives, 15).unwrap();

    let result = Rules::score(Category::Sixes, dice([6, 6, 6, 6, 6]), &sheet);
    assert_eq!(result.base_score, 30);
    assert_eq!(result.upper_bonus, 35);
    assert_eq!(result.total_delta, 65);
}

#[test]
fn joker_forces_matching_upper_category_when_open() {
    let mut sheet = ScoreSheet::new();
    sheet.fill_raw(Category::Yahtzee, 50).unwrap();
    let legal = Rules::legal_score_categories(&sheet, dice([6, 6, 6, 6, 6]));
    assert_eq!(legal, vec![Category::Sixes]);
}

#[test]
fn joker_allows_lower_categories_when_matching_upper_is_filled() {
    let mut sheet = ScoreSheet::new();
    sheet.fill_raw(Category::Yahtzee, 50).unwrap();
    sheet.fill_raw(Category::Sixes, 30).unwrap();

    let legal = Rules::legal_score_categories(&sheet, dice([6, 6, 6, 6, 6]));
    assert!(legal.contains(&Category::FullHouse));
    assert!(legal.contains(&Category::SmallStraight));
    assert!(!legal.contains(&Category::Ones));

    let result = Rules::score(Category::LargeStraight, dice([6, 6, 6, 6, 6]), &sheet);
    assert_eq!(result.base_score, 40);
    assert_eq!(result.yahtzee_bonus, 100);
}

#[test]
fn zeroed_yahtzee_does_not_enable_joker() {
    let mut sheet = ScoreSheet::new();
    sheet.fill_raw(Category::Yahtzee, 0).unwrap();

    let legal = Rules::legal_score_categories(&sheet, dice([6, 6, 6, 6, 6]));
    assert!(legal.contains(&Category::Sixes));
    assert!(legal.contains(&Category::Chance));
    assert_eq!(
        Rules::score(Category::FullHouse, dice([6, 6, 6, 6, 6]), &sheet).base_score,
        0
    );
}

#[test]
fn buddyboardgames_variant_allows_zeroed_yahtzee_wildcard() {
    let mut sheet = ScoreSheet::new();
    sheet.fill_raw(Category::Yahtzee, 0).unwrap();

    let legal = Rules::legal_score_categories_with_ruleset(
        Ruleset::BuddyBoardGames,
        &sheet,
        dice([6, 6, 6, 6, 6]),
    );
    assert_eq!(legal, sheet.remaining_categories());
    assert_eq!(
        Rules::score_with_ruleset(
            Ruleset::BuddyBoardGames,
            Category::FullHouse,
            dice([6, 6, 6, 6, 6]),
            &sheet,
        )
        .base_score,
        0
    );

    sheet.fill_raw(Category::Sixes, 0).unwrap();
    let result = Rules::score_with_ruleset(
        Ruleset::BuddyBoardGames,
        Category::FullHouse,
        dice([6, 6, 6, 6, 6]),
        &sheet,
    );
    assert_eq!(result.base_score, 25);
    assert_eq!(result.yahtzee_bonus, 0);
}

#[test]
fn joker_falls_back_to_upper_categories_when_lower_section_is_full() {
    let mut sheet = ScoreSheet::new();
    for category in Category::LOWER {
        let score = if category == Category::Yahtzee { 50 } else { 0 };
        sheet.fill_raw(category, score).unwrap();
    }
    sheet.fill_raw(Category::Sixes, 30).unwrap();

    let legal = Rules::legal_score_categories(&sheet, dice([6, 6, 6, 6, 6]));
    assert_eq!(
        legal,
        vec![
            Category::Ones,
            Category::Twos,
            Category::Threes,
            Category::Fours,
            Category::Fives
        ]
    );
}

#[test]
fn action_generation_respects_turn_phase() {
    let start = GameState::new();
    assert_eq!(
        Rules::legal_actions(&start),
        vec![Action::Roll { hold_mask: 0 }]
    );

    let rolled = GameState::from_parts(Some(dice([1, 2, 3, 4, 5])), 2, ScoreSheet::new()).unwrap();
    let actions = Rules::legal_actions(&rolled);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, Action::Roll { .. }))
            .count(),
        32
    );
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, Action::Score { .. }))
            .count(),
        13
    );

    let final_roll =
        GameState::from_parts(Some(dice([1, 2, 3, 4, 5])), 3, ScoreSheet::new()).unwrap();
    assert!(
        Rules::legal_actions(&final_roll)
            .iter()
            .all(|action| matches!(action, Action::Score { .. }))
    );
}

#[test]
fn compact_state_parser_builds_reusable_game_state() {
    let state = GameState::parse_compact(
        "dice=6,5,4,3,2 rolls=2 scores=ones:3,twos:6,full-house:25 yahtzee_bonus=1",
    )
    .unwrap();

    assert_eq!(state.dice().unwrap().values(), [2, 3, 4, 5, 6]);
    assert_eq!(state.rolls_used(), 2);
    assert_eq!(state.sheet().score(Category::Ones), Some(3));
    assert_eq!(state.sheet().score(Category::Twos), Some(6));
    assert_eq!(state.sheet().score(Category::FullHouse), Some(25));
    assert_eq!(state.sheet().yahtzee_bonus_count(), 1);
}

#[test]
fn compact_state_parser_rejects_impossible_recorded_scores() {
    assert!(GameState::parse_compact("dice=1,2,3,4,5 rolls=1 scores=twos:3").is_err());
    assert!(GameState::parse_compact("dice=1,2,3,4,5 rolls=1 scores=full-house:20").is_err());
    assert!(GameState::parse_compact("dice=1,2,3,4,5 rolls=1 scores=yahtzee:49").is_err());
}

#[test]
fn simulator_is_deterministic_and_completes_a_full_game() {
    let mut first = GameSimulator::new(42, Ruleset::HasbroStrict, 2);
    let mut second = GameSimulator::new(42, Ruleset::HasbroStrict, 2);

    let first_report = first.simulate(true).unwrap();
    let second_report = second.simulate(true).unwrap();

    assert_eq!(first_report.final_score, second_report.final_score);
    assert_eq!(first_report.turn_log, second_report.turn_log);
    assert_eq!(first_report.turn_count, 13);
    assert!(first_report.final_score <= 1575);
}

#[test]
fn simulator_seeds_change_roll_sequences() {
    let mut first = GameSimulator::new(1, Ruleset::HasbroStrict, 2);
    let mut second = GameSimulator::new(2, Ruleset::HasbroStrict, 2);

    let first_report = first.simulate(true).unwrap();
    let second_report = second.simulate(true).unwrap();

    assert_ne!(first_report.turn_log, second_report.turn_log);
}

#[test]
fn hybrid_agent_uses_oracle_at_configured_endgame_threshold() {
    let mut sheet = ScoreSheet::new();
    for category in Category::ALL {
        if category != Category::Chance {
            sheet.fill_raw(category, 0).unwrap();
        }
    }
    let state = GameState::from_parts(Some(dice([1, 2, 3, 4, 6])), 3, sheet).unwrap();
    let agent = HybridAgent::new(Ruleset::HasbroStrict, 2);

    assert!(agent.uses_oracle_for(&state));
}

#[test]
fn buddyboardgames_row_mapping_round_trips_selectable_categories() {
    for category in Category::ALL {
        let row = bbg_category_to_client_row(category).unwrap();
        assert_eq!(bbg_client_row_to_category(row), Some(category));
    }
    assert_eq!(bbg_client_row_to_category(6), None);
    assert_eq!(bbg_client_row_to_category(14), None);
}

#[test]
fn buddyboardgames_snapshot_rejects_unsafe_or_malformed_states() {
    let rows = "rows=0:3:1,1:6:1";
    let base = format!(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=1,2,3,4,5 selected=0,0,0,0,0 rolls=2 {rows}"
    );
    assert!(BuddyBoardGamesSnapshot::parse_compact(&base).is_ok());

    assert!(BuddyBoardGamesSnapshot::parse_compact(&base.replace("turn=0", "turn=1")).is_err());
    assert!(
        BuddyBoardGamesSnapshot::parse_compact(&base.replace("spectator=false", "spectator=true"))
            .is_err()
    );
    assert!(
        BuddyBoardGamesSnapshot::parse_compact(&base.replace("pending=false", "pending=true"))
            .is_err()
    );
    assert!(BuddyBoardGamesSnapshot::parse_compact(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=1,2,3 selected=0,0,0,0,0 rolls=2 rows=0:3:1"
    )
    .is_err());
}

#[test]
fn buddyboardgames_snapshot_normalizes_yahtzee_bonus_row_total() {
    let snapshot = BuddyBoardGamesSnapshot::parse_compact(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=1,1,1,1,1 selected=0,0,0,0,0 rolls=0 rows=12:150:1,0:3:1",
    )
    .unwrap();

    let state = snapshot.to_game_state().unwrap();

    assert_eq!(state.sheet().score(Category::Yahtzee), Some(50));
    assert_eq!(state.sheet().yahtzee_bonus_count(), 1);
}

#[test]
fn buddyboardgames_advice_maps_actions_to_site_selectors() {
    let snapshot = BuddyBoardGamesSnapshot::parse_compact(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=1,2,3,4,5 selected=0,0,0,0,0 rolls=3 rows=0:3:1,1:6:1,2:9:1,3:12:1,4:15:1,5:18:1,7:20:1,8:0:1,9:25:1,10:30:1,11:40:1,12:0:1",
    )
    .unwrap();

    let advice = advise_buddyboardgames_snapshot(&snapshot, 2).unwrap();
    assert_eq!(advice.source, DecisionSource::Heuristic);
    assert_eq!(
        advice.action,
        Action::Score {
            category: Category::Chance
        }
    );
    assert_eq!(advice.client_row, Some(13));
    assert_eq!(advice.selector, "#player-0-scoreboard-row-13");
}

#[test]
fn exact_buddyboardgames_advice_reports_exact_source() {
    let snapshot = BuddyBoardGamesSnapshot::parse_compact(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=1,2,3,4,5 selected=0,0,0,0,0 rolls=3 rows=0:3:1,1:6:1,2:9:1,3:12:1,4:15:1,5:18:1,7:20:1,8:0:1,9:25:1,10:30:1,11:40:1,12:0:1,13:0:0",
    )
    .unwrap();
    let table = AnchorValueTable::build_limited(Ruleset::BuddyBoardGames, 0).unwrap();

    let advice = advise_buddyboardgames_snapshot_exact(&snapshot, table, 3).unwrap();

    assert_eq!(advice.source, DecisionSource::ExactTable);
    assert_eq!(
        advice.action,
        Action::Score {
            category: Category::Chance
        }
    );
    assert_eq!(advice.client_row, Some(13));
}

#[test]
fn buddyboardgames_roll_advice_maps_canonical_hold_to_page_dice() {
    let snapshot = BuddyBoardGamesSnapshot::parse_compact(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=6,2,2,6,5 selected=0,0,0,0,0 rolls=1 rows=0:0:0",
    )
    .unwrap();

    let advice = keiri::BuddyBoardGamesAdvice::from_snapshot(
        &snapshot,
        Action::Roll { hold_mask: 0b00101 },
        Some(7.407407),
        DecisionSource::ExactTable,
        snapshot.to_game_state().unwrap().to_compact(),
        Vec::new(),
    )
    .unwrap();

    assert_eq!(snapshot.dice.values(), [2, 2, 5, 6, 6]);
    assert_eq!(snapshot.page_dice, [6, 2, 2, 6, 5]);
    assert_eq!(advice.hold_mask, Some(0b00101));
    assert_eq!(advice.page_hold_mask, Some(0b10010));
    assert_eq!(advice.toggle_dice, vec![1, 4]);

    let cli = advice.to_cli_lines();
    assert!(cli.contains("hold_mask: 00101"));
    assert!(cli.contains("page_hold_mask: 10010"));
    assert!(cli.contains("toggle_dice: 1,4"));
}

#[test]
fn buddyboardgames_roll_advice_prefers_selected_matching_duplicates() {
    let snapshot = BuddyBoardGamesSnapshot::parse_compact(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=6,2,2,6,5 selected=0,0,1,0,1 rolls=1 rows=0:0:0",
    )
    .unwrap();

    let advice = keiri::BuddyBoardGamesAdvice::from_snapshot(
        &snapshot,
        Action::Roll { hold_mask: 0b00101 },
        None,
        DecisionSource::ExactTable,
        snapshot.to_game_state().unwrap().to_compact(),
        Vec::new(),
    )
    .unwrap();

    assert_eq!(advice.hold_mask, Some(0b00101));
    assert_eq!(advice.page_hold_mask, Some(0b10100));
    assert!(advice.toggle_dice.is_empty());
}

#[test]
fn buddyboardgames_roll_advice_uses_earliest_unselected_duplicate() {
    let snapshot = BuddyBoardGamesSnapshot::parse_compact(
        "state=STARTED me=0 turn=0 spectator=false pending=false dice=6,2,2,6,5 selected=0,0,0,0,0 rolls=1 rows=0:0:0",
    )
    .unwrap();

    let advice = keiri::BuddyBoardGamesAdvice::from_snapshot(
        &snapshot,
        Action::Roll { hold_mask: 0b00001 },
        None,
        DecisionSource::ExactTable,
        snapshot.to_game_state().unwrap().to_compact(),
        Vec::new(),
    )
    .unwrap();

    assert_eq!(advice.hold_mask, Some(0b00001));
    assert_eq!(advice.page_hold_mask, Some(0b00010));
    assert_eq!(advice.toggle_dice, vec![1]);
}

#[test]
fn cli_simulate_returns_numeric_score() {
    let output = Command::new(env!("CARGO_BIN_EXE_keiri"))
        .arg("simulate")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.trim().parse::<u16>().is_ok());
}

#[test]
fn cli_dash_dash_simulate_alias_returns_numeric_score() {
    let output = Command::new(env!("CARGO_BIN_EXE_keiri"))
        .arg("--simulate")
        .arg("seed=42")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.trim().parse::<u16>().is_ok());
}

#[test]
fn cli_evaluate_writes_summary_and_score_csvs() {
    let base = std::env::temp_dir().join(format!("keiri-eval-{}", std::process::id()));
    let history = base.join("history.csv");
    let scores = base.join("scores.csv");
    let _ = std::fs::remove_dir_all(&base);

    let output = Command::new(env!("CARGO_BIN_EXE_keiri"))
        .arg("evaluate")
        .arg("games=3")
        .arg("seed=1")
        .arg("oracle_endgame=0")
        .arg(format!("out={}", history.display()))
        .arg(format!("scores_out={}", scores.display()))
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("mean:"));
    assert!(
        std::fs::read_to_string(&history)
            .unwrap()
            .starts_with("timestamp_unix,agent,rules,games")
    );
    assert_eq!(std::fs::read_to_string(&scores).unwrap().lines().count(), 4);

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn cli_buddyboardgames_defaults_to_exact_for_simulate_and_evaluate_when_table_exists() {
    let Ok(table) = AnchorValueTable::load(BBG_FULL_TABLE_PATH) else {
        return;
    };
    let Ok(value) = table.expected_value(&GameState::new()) else {
        return;
    };
    if (value - BBG_OPENING_EXPECTED_VALUE).abs() > BBG_OPENING_TOLERANCE {
        return;
    }

    let simulate = Command::new(env!("CARGO_BIN_EXE_keiri"))
        .arg("simulate")
        .arg("rules=buddyboardgames")
        .arg("seed=1")
        .arg("verbose=true")
        .arg(format!("table={BBG_FULL_TABLE_PATH}"))
        .output()
        .unwrap();
    assert!(simulate.status.success());
    let stdout = String::from_utf8(simulate.stdout).unwrap();
    assert!(stdout.contains("agent: auto"));

    let evaluate = Command::new(env!("CARGO_BIN_EXE_keiri"))
        .arg("evaluate")
        .arg("rules=buddyboardgames")
        .arg("games=1")
        .arg("seed=1")
        .arg(format!("table={BBG_FULL_TABLE_PATH}"))
        .arg("out=none")
        .output()
        .unwrap();
    assert!(evaluate.status.success());
    let stdout = String::from_utf8(evaluate.stdout).unwrap();
    assert!(stdout.contains("agent: auto"));
}

#[test]
fn canonical_dice_enumeration_has_all_sorted_outcomes() {
    let dice = Dice::all_canonical();
    assert_eq!(dice.len(), 252);
    assert_eq!(dice.first().unwrap().values(), [1, 1, 1, 1, 1]);
    assert_eq!(dice.last().unwrap().values(), [6, 6, 6, 6, 6]);
    assert!(
        dice.iter()
            .all(|dice| dice.values().windows(2).all(|pair| pair[0] <= pair[1]))
    );
}

#[test]
fn endgame_table_builder_emits_expected_rows_and_tsv() {
    let sheet = sheet_filled_except(Category::Chance);
    let table = OracleTable::build_endgame(sheet, &[3]).unwrap();

    assert_eq!(table.rows().len(), 252);
    assert_eq!(
        table.rows()[0].state.dice().unwrap().values(),
        [1, 1, 1, 1, 1]
    );
    assert_eq!(
        table.rows()[0].best_action,
        Some(Action::Score {
            category: Category::Chance
        })
    );

    let tsv = table.to_tsv();
    assert!(tsv.starts_with("state\taction\texpected_value\n"));
    assert!(tsv.contains("dice=1,1,1,1,1 rolls=3"));
    assert!(tsv.contains("\tscore chance\t105.000000\n"));
}

#[test]
fn endgame_table_builder_rejects_unbounded_slices() {
    assert!(OracleTable::build_endgame(ScoreSheet::new(), &[3]).is_err());

    let sheet = sheet_filled_except(Category::Chance);
    assert!(OracleTable::build_endgame(sheet.clone(), &[]).is_err());
    assert!(OracleTable::build_endgame(sheet, &[0]).is_err());
}

#[test]
fn scoring_is_bounded_for_all_ordered_rolls() {
    let sheet = ScoreSheet::new();
    for a in 1..=6 {
        for b in 1..=6 {
            for c in 1..=6 {
                for d in 1..=6 {
                    for e in 1..=6 {
                        let dice = dice([a, b, c, d, e]);
                        for category in Category::ALL {
                            let score = Rules::score(category, dice, &sheet).base_score;
                            assert!(
                                score <= Rules::max_base_score(category),
                                "{category} scored {score} for {dice}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn score_transition_preserves_sheet_invariants() {
    let state = GameState::from_parts(Some(dice([1, 2, 3, 4, 5])), 3, ScoreSheet::new()).unwrap();
    let before_total = state.sheet().total_score();
    let before_filled = state.sheet().filled_count();

    let next = Rules::apply_score(&state, Category::LargeStraight).unwrap();

    assert_eq!(next.sheet().filled_count(), before_filled + 1);
    assert!(next.sheet().is_filled(Category::LargeStraight));
    assert!(next.sheet().total_score() >= before_total);
    assert_eq!(next.dice(), None);
    assert_eq!(next.rolls_used(), 0);
}

#[test]
fn transition_rejects_invalid_rolls_and_scores() {
    let state = GameState::from_parts(Some(dice([1, 2, 3, 4, 5])), 3, ScoreSheet::new()).unwrap();
    assert!(Rules::apply_roll(&state, 0, &[1, 2, 3, 4, 5]).is_err());

    let mut sheet = ScoreSheet::new();
    sheet.fill_raw(Category::Chance, 20).unwrap();
    let state = GameState::from_parts(Some(dice([1, 2, 3, 4, 5])), 1, sheet).unwrap();
    assert!(Rules::apply_score(&state, Category::Chance).is_err());
}

#[test]
fn reroll_distribution_weights_match_outcome_counts() {
    let agent = OptimalAgent::new();
    for dice_count in 0..=5 {
        let weight_sum: u32 = agent
            .reroll_distribution(dice_count)
            .unwrap()
            .iter()
            .map(|(_, weight)| *weight)
            .sum();
        assert_eq!(weight_sum, 6u32.pow(dice_count as u32));
    }
}

#[test]
fn oracle_scores_obvious_single_category_endgame() {
    let sheet = sheet_filled_except(Category::Chance);
    let state = GameState::from_parts(Some(dice([1, 2, 3, 4, 6])), 3, sheet).unwrap();
    let mut agent = OptimalAgent::new();

    let decision = agent.best_action(&state).unwrap();
    assert_eq!(
        decision.action,
        Action::Score {
            category: Category::Chance
        }
    );
    assert_eq!(decision.expected_value, 16.0);
}

#[test]
fn oracle_cache_reuse_preserves_value() {
    let sheet = sheet_filled_except(Category::Chance);
    let state = GameState::from_parts(Some(dice([1, 2, 3, 4, 6])), 2, sheet).unwrap();
    let mut agent = OptimalAgent::new();

    let first = agent.expected_value(&state);
    let cache_len = agent.cache_len();
    let second = agent.expected_value(&state);
    assert_eq!(first, second);
    assert!(first > 16.0);
    assert_eq!(cache_len, agent.cache_len());
}

#[test]
fn exact_table_matches_recursive_oracle_for_one_open_final_rolls() {
    let table = AnchorValueTable::build_limited(Ruleset::HasbroStrict, 0).unwrap();

    for open in Category::ALL {
        let sheet = sheet_filled_except(open);
        for dice in Dice::all_canonical() {
            let state = GameState::from_parts(Some(dice), 3, sheet.clone()).unwrap();
            let mut oracle = OptimalAgent::new();
            let exact = table.best_action(&state).unwrap().unwrap();
            let recursive = oracle.best_action(&state).unwrap();

            assert_eq!(exact.action, recursive.action, "{open} {dice}");
            assert!(
                (exact.expected_value - recursive.expected_value).abs() < 1e-9,
                "{open} {dice}: exact={} recursive={}",
                exact.expected_value,
                recursive.expected_value
            );
        }
    }
}

#[test]
fn exact_table_matches_recursive_oracle_for_sample_two_open_states() {
    let mut table = AnchorValueTable::build_limited(Ruleset::HasbroStrict, 0).unwrap();
    let samples = [
        (
            [Category::Chance, Category::Yahtzee],
            dice([1, 2, 3, 4, 6]),
            3,
        ),
        (
            [Category::Sixes, Category::LargeStraight],
            dice([2, 3, 4, 5, 6]),
            3,
        ),
        (
            [Category::FourKind, Category::FullHouse],
            dice([5, 5, 5, 2, 2]),
            3,
        ),
    ];

    for (open_categories, dice, rolls) in samples {
        let mut sheet = ScoreSheet::new();
        for category in Category::ALL {
            if !open_categories.contains(&category) {
                let score = if category == Category::Yahtzee { 50 } else { 0 };
                sheet.fill_raw(category, score).unwrap();
            }
        }
        let state = GameState::from_parts(Some(dice), rolls, sheet).unwrap();
        let mut oracle = OptimalAgent::new();
        for category in Rules::legal_score_categories(state.sheet(), dice) {
            let next = Rules::apply_score(&state, category).unwrap();
            let future = oracle.expected_value(&next);
            table.set_value_for_sheet(next.sheet(), future);
        }
        let exact = table.best_action(&state).unwrap().unwrap();
        let recursive = oracle.best_action(&state).unwrap();

        assert!(
            (exact.expected_value - recursive.expected_value).abs() < 1e-9,
            "{state:?}: exact={} recursive={}",
            exact.expected_value,
            recursive.expected_value
        );
    }
}

#[test]
fn exact_table_round_trips_serialized_values_and_rejects_tampering() {
    let table = AnchorValueTable::build_limited(Ruleset::BuddyBoardGames, 0).unwrap();
    let bytes = table.to_bytes();
    let loaded = AnchorValueTable::from_bytes(&bytes).unwrap();
    assert_eq!(loaded.ruleset(), Ruleset::BuddyBoardGames);

    let state = GameState::from_parts(
        Some(dice([6, 6, 6, 6, 6])),
        3,
        sheet_filled_except(Category::Sixes),
    )
    .unwrap();
    assert_eq!(
        table.best_action(&state).unwrap().unwrap().expected_value,
        loaded.best_action(&state).unwrap().unwrap().expected_value
    );

    let mut tampered = bytes;
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(AnchorValueTable::from_bytes(&tampered).is_err());

    let mut stale_version = table.to_bytes();
    stale_version[8..12].copy_from_slice(&1u32.to_le_bytes());
    assert!(AnchorValueTable::from_bytes(&stale_version).is_err());
}

#[test]
fn dense_anchor_builder_matches_recursive_oracle_for_one_open_start_states() {
    let table = AnchorValueTable::build_limited_with_options(
        Ruleset::HasbroStrict,
        1,
        AnchorBuildOptions {
            threads: Some(2),
            strategy: AnchorBuildStrategy::Dense,
        },
    )
    .unwrap();

    for open in Category::ALL {
        let sheet = sheet_filled_except(open);
        let state = GameState::from_parts(None, 0, sheet.clone()).unwrap();
        let mut oracle = OptimalAgent::new();
        let dense = table.value_for_sheet(&sheet).unwrap();
        let recursive = oracle.expected_value(&state);

        assert!(
            (dense - recursive).abs() < 1e-9,
            "{open}: dense={dense} recursive={recursive}"
        );
    }
}

#[test]
fn dense_anchor_builder_matches_recursive_oracle_for_sample_two_open_states() {
    let table = AnchorValueTable::build_limited_with_options(
        Ruleset::HasbroStrict,
        2,
        AnchorBuildOptions {
            threads: Some(2),
            strategy: AnchorBuildStrategy::Dense,
        },
    )
    .unwrap();
    let samples = [
        (
            [Category::Chance, Category::Yahtzee],
            dice([1, 2, 3, 4, 6]),
            2,
        ),
        (
            [Category::Sixes, Category::LargeStraight],
            dice([2, 3, 4, 5, 6]),
            1,
        ),
        (
            [Category::FourKind, Category::FullHouse],
            dice([5, 5, 5, 2, 2]),
            3,
        ),
    ];

    for (open_categories, dice, rolls) in samples {
        let mut sheet = ScoreSheet::new();
        for category in Category::ALL {
            if !open_categories.contains(&category) {
                let score = if category == Category::Yahtzee { 50 } else { 0 };
                sheet.fill_raw(category, score).unwrap();
            }
        }
        let state = GameState::from_parts(Some(dice), rolls, sheet).unwrap();
        let exact = table.best_action(&state).unwrap().unwrap();
        let mut oracle = OptimalAgent::new();
        let recursive = oracle.best_action(&state).unwrap();

        assert!(
            (exact.expected_value - recursive.expected_value).abs() < 1e-9,
            "{state:?}: exact={} recursive={}",
            exact.expected_value,
            recursive.expected_value
        );
    }
}

#[test]
fn exact_bbg_table_applies_zeroed_yahtzee_wildcard() {
    let table = AnchorValueTable::build_limited(Ruleset::BuddyBoardGames, 0).unwrap();
    let mut sheet = ScoreSheet::new();
    for category in Category::ALL {
        if category != Category::FullHouse {
            sheet.fill_raw(category, 0).unwrap();
        }
    }
    let state = GameState::from_parts(Some(dice([6, 6, 6, 6, 6])), 3, sheet).unwrap();
    let exact = table.best_action(&state).unwrap().unwrap();

    assert_eq!(
        exact.action,
        Action::Score {
            category: Category::FullHouse
        }
    );
    assert_eq!(exact.expected_value, 25.0);
}

#[test]
fn full_bbg_table_opening_expected_value_matches_known_optimum_when_available() {
    let Ok(table) = AnchorValueTable::load(BBG_FULL_TABLE_PATH) else {
        return;
    };
    let value = table.expected_value(&GameState::new()).unwrap();

    assert!(
        (value - BBG_OPENING_EXPECTED_VALUE).abs() <= BBG_OPENING_TOLERANCE,
        "opening expected value {value:.6} did not match {BBG_OPENING_EXPECTED_VALUE:.6}"
    );
}

#[test]
fn dice_permutation_does_not_affect_scoring() {
    let sheet = ScoreSheet::new();
    let a = dice([2, 3, 2, 3, 3]);
    let b = dice([3, 2, 3, 2, 3]);

    for category in Category::ALL {
        assert_eq!(
            Rules::score(category, a, &sheet).base_score,
            Rules::score(category, b, &sheet).base_score
        );
    }
}
