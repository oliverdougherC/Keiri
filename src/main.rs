use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use keiri::{
    Agent, AnchorBuildOptions, AnchorBuildProgress, AnchorBuildStrategy, AnchorValueTable,
    BuddyBoardGamesSnapshot, Category, Dice, ExactTableAgent, GameSimulator, GameState, KeiriError,
    OptimalAgent, OracleTable, Rules, Ruleset, ScoreSheet, advise_buddyboardgames_snapshot,
    advise_buddyboardgames_snapshot_exact, simulate_with_agent,
};

const BBG_OPENING_EXPECTED_VALUE: f64 = 254.5896;
const OPENING_EXPECTED_VALUE_TOLERANCE: f64 = 0.001;

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("error: {error}");
        eprintln!();
        print_usage();
        process::exit(2);
    }
}

fn run(args: Vec<String>) -> Result<(), KeiriError> {
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Ok(());
    };

    match command {
        "--simulate" | "simulate" => simulate_command(&args[1..]),
        "--bbg-join" | "bbg-join" => bbg_join_command(&args[1..]),
        "evaluate" | "eval" => evaluate_command(&args[1..]),
        "score" => score_command(&args[1..]),
        "actions" => actions_command(&args[1..]),
        "advise" => advise_command(&args[1..]),
        "bbg-advise" => bbg_advise_command(&args[1..]),
        "build-table" => build_table_command(&args[1..]),
        "build-anchor-table" => build_anchor_table_command(&args[1..]),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        other => Err(KeiriError::ParseError(format!("unknown command `{other}`"))),
    }
}

fn bbg_join_command(args: &[String]) -> Result<(), KeiriError> {
    let mut room = None;
    let mut player = "keiri-bot".to_string();
    let mut url = None;
    let mut start_game = false;
    let mut keep_open = true;
    let mut headed = true;
    let mut play = true;
    let mut max_actions = None;
    let mut poll_ms = 1000u64;
    let mut oracle_endgame = 2usize;
    let mut agent = CliAgent::Auto;
    let mut table = None;

    for arg in args {
        if let Some(value) = arg.strip_prefix("room=") {
            room = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("code=") {
            room = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("player=") {
            player = value.to_string();
        } else if let Some(value) = arg.strip_prefix("url=") {
            url = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("start=") {
            start_game = parse_bool_option("start", value)?;
        } else if let Some(value) = arg.strip_prefix("keep_open=") {
            keep_open = parse_bool_option("keep_open", value)?;
        } else if let Some(value) = arg.strip_prefix("headed=") {
            headed = parse_bool_option("headed", value)?;
        } else if let Some(value) = arg.strip_prefix("play=") {
            play = parse_bool_option("play", value)?;
        } else if let Some(value) = arg.strip_prefix("max_actions=") {
            max_actions =
                Some(value.parse::<usize>().map_err(|_| {
                    KeiriError::ParseError(format!("invalid max_actions `{value}`"))
                })?);
        } else if let Some(value) = arg.strip_prefix("poll_ms=") {
            poll_ms = value
                .parse::<u64>()
                .map_err(|_| KeiriError::ParseError(format!("invalid poll_ms `{value}`")))?;
        } else if let Some(value) = arg.strip_prefix("oracle_endgame=") {
            oracle_endgame = value
                .parse::<usize>()
                .map_err(|_| KeiriError::ParseError(format!("invalid oracle_endgame `{value}`")))?;
        } else if let Some(value) = arg.strip_prefix("agent=") {
            agent = parse_agent(value)?;
        } else if let Some(value) = arg.strip_prefix("table=") {
            table = Some(value.to_string());
        } else if arg.contains('=') {
            return Err(KeiriError::ParseError(format!(
                "unknown bbg-join option `{arg}`"
            )));
        } else if room.is_none() {
            room = Some(arg.to_string());
        } else {
            return Err(KeiriError::ParseError(format!(
                "unexpected bbg-join argument `{arg}`"
            )));
        }
    }

    let room = match room {
        Some(room) if !room.trim().is_empty() => room,
        _ => prompt_for_room_code()?,
    };

    if player.trim().is_empty() {
        return Err(KeiriError::ParseError(
            "player name must not be empty".to_string(),
        ));
    }

    ensure_node_dependencies()?;

    let mut command = process::Command::new("node");
    command.arg("tools/buddyboardgames/autoplay.mjs");
    if play {
        command.arg("--loop").arg("--execute");
    } else {
        command.arg("--join-only");
    }
    command.arg(format!("--room={room}"));
    command.arg(format!("--player={player}"));
    command.arg(format!("--poll-ms={poll_ms}"));
    command.arg(format!("--oracle-endgame={oracle_endgame}"));
    let table = table.or_else(|| {
        matches!(agent, CliAgent::Auto | CliAgent::ExactTable)
            .then(|| default_anchor_table_path(Ruleset::BuddyBoardGames))
    });
    if matches!(agent, CliAgent::Auto | CliAgent::ExactTable) {
        let table_path = table
            .as_deref()
            .unwrap_or("target/keiri_tables/bbg-anchor-v1.bin");
        ensure_anchor_table(table_path, Ruleset::BuddyBoardGames)?;
    }
    command.arg(format!("--agent={}", agent.as_cli_value()));
    if let Some(table) = table {
        command.arg(format!("--table={table}"));
    }
    if let Some(max_actions) = max_actions {
        command.arg(format!("--max-actions={max_actions}"));
    }
    if let Some(url) = url {
        command.arg(format!("--url={url}"));
    }
    if start_game {
        command.arg("--start-game");
    }
    if play {
        command.arg("--auto-start-solo");
    }
    if keep_open {
        command.arg("--keep-open");
    }
    if headed {
        command.arg("--headed");
    }

    let status = command.status().map_err(|error| {
        KeiriError::ParseError(format!(
            "failed to launch BuddyBoardGames join helper: {error}"
        ))
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(KeiriError::ParseError(format!(
            "BuddyBoardGames join helper exited with {status}"
        )))
    }
}

fn simulate_command(args: &[String]) -> Result<(), KeiriError> {
    let mut seed = None;
    let mut verbose = false;
    let mut ruleset = Ruleset::HasbroStrict;
    let mut oracle_endgame = 2;
    let mut agent = None;
    let mut table = None;

    for arg in args {
        let (key, value) = arg.split_once('=').ok_or_else(|| {
            KeiriError::ParseError(format!("simulate option `{arg}` must be key=value"))
        })?;
        match key {
            "seed" => {
                seed = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| KeiriError::ParseError(format!("invalid seed `{value}`")))?,
                );
            }
            "verbose" => verbose = parse_bool_option("verbose", value)?,
            "rules" => {
                ruleset = Ruleset::from_name(value)
                    .ok_or_else(|| KeiriError::ParseError(format!("unknown ruleset `{value}`")))?;
            }
            "oracle_endgame" => {
                oracle_endgame = value.parse::<usize>().map_err(|_| {
                    KeiriError::ParseError(format!("invalid oracle_endgame `{value}`"))
                })?;
                if oracle_endgame > Category::ALL.len() {
                    return Err(KeiriError::ParseError(format!(
                        "oracle_endgame must be 0..={}",
                        Category::ALL.len()
                    )));
                }
            }
            "agent" => agent = Some(parse_agent(value)?),
            "table" => table = Some(value.to_string()),
            other => {
                return Err(KeiriError::ParseError(format!(
                    "unknown simulate option `{other}`"
                )));
            }
        }
    }

    let seed = seed.unwrap_or_else(generate_seed);
    let agent = agent.unwrap_or_else(|| default_agent_for_ruleset(ruleset));
    let report = simulate_report(
        seed,
        ruleset,
        oracle_endgame,
        agent,
        table.as_deref(),
        verbose,
    )?;
    if verbose {
        println!("agent: {}", agent.as_cli_value());
        println!("score: {}", report.final_score);
        println!("seed: {}", report.seed);
        println!("turns: {}", report.turn_count);
        println!("upper_bonus: {}", report.upper_bonus);
        println!("yahtzee_bonus_count: {}", report.yahtzee_bonus_count);
        for line in report.turn_log {
            println!("{line}");
        }
    } else {
        println!("{}", report.final_score);
    }
    Ok(())
}

fn prompt_for_room_code() -> Result<String, KeiriError> {
    print!("BuddyBoardGames room code: ");
    io::stdout()
        .flush()
        .map_err(|error| KeiriError::ParseError(format!("failed to flush stdout: {error}")))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| KeiriError::ParseError(format!("failed to read room code: {error}")))?;
    let room = input.trim().to_string();
    if room.is_empty() {
        Err(KeiriError::ParseError(
            "room code must not be empty".to_string(),
        ))
    } else {
        Ok(room)
    }
}

fn ensure_node_dependencies() -> Result<(), KeiriError> {
    if Path::new("node_modules/playwright").exists() {
        return Ok(());
    }
    println!("Installing Node tooling for BuddyBoardGames integration...");
    let status = process::Command::new("npm")
        .arg("install")
        .status()
        .map_err(|error| KeiriError::ParseError(format!("failed to run npm install: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(KeiriError::ParseError(format!(
            "npm install exited with {status}"
        )))
    }
}

fn evaluate_command(args: &[String]) -> Result<(), KeiriError> {
    let mut games = 100;
    let mut seed = 1;
    let mut ruleset = Ruleset::HasbroStrict;
    let mut oracle_endgame = 0;
    let mut agent = None;
    let mut table = None;
    let mut out = Some("metrics/simulation_history.csv".to_string());
    let mut scores_out = None;
    let mut append = true;

    for arg in args {
        let (key, value) = arg.split_once('=').ok_or_else(|| {
            KeiriError::ParseError(format!("evaluate option `{arg}` must be key=value"))
        })?;
        match key {
            "games" => {
                games = value
                    .parse::<usize>()
                    .map_err(|_| KeiriError::ParseError(format!("invalid games `{value}`")))?;
                if games == 0 {
                    return Err(KeiriError::ParseError("games must be > 0".to_string()));
                }
            }
            "seed" => {
                seed = value
                    .parse::<u64>()
                    .map_err(|_| KeiriError::ParseError(format!("invalid seed `{value}`")))?;
            }
            "rules" => {
                ruleset = Ruleset::from_name(value)
                    .ok_or_else(|| KeiriError::ParseError(format!("unknown ruleset `{value}`")))?;
            }
            "oracle_endgame" => {
                oracle_endgame = value.parse::<usize>().map_err(|_| {
                    KeiriError::ParseError(format!("invalid oracle_endgame `{value}`"))
                })?;
                if oracle_endgame > Category::ALL.len() {
                    return Err(KeiriError::ParseError(format!(
                        "oracle_endgame must be 0..={}",
                        Category::ALL.len()
                    )));
                }
            }
            "agent" => agent = Some(parse_agent(value)?),
            "table" => table = Some(value.to_string()),
            "out" => {
                out = if matches!(value, "none" | "-") {
                    None
                } else {
                    Some(value.to_string())
                };
            }
            "scores_out" => scores_out = Some(value.to_string()),
            "append" => append = parse_bool_option("append", value)?,
            other => {
                return Err(KeiriError::ParseError(format!(
                    "unknown evaluate option `{other}`"
                )));
            }
        }
    }

    let agent = agent.unwrap_or_else(|| default_agent_for_ruleset(ruleset));
    let summary = run_evaluation(
        games,
        seed,
        ruleset,
        oracle_endgame,
        agent,
        table.as_deref(),
    )?;
    println!("{}", summary.to_cli_lines());

    if let Some(path) = out {
        write_evaluation_summary(&path, &summary, append)?;
        println!("history: {path}");
    }
    if let Some(path) = scores_out {
        write_score_series(&path, &summary)?;
        println!("scores: {path}");
    }

    Ok(())
}

fn generate_seed() -> u64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let nanos = duration.as_nanos() as u64;
    nanos ^ u64::from(process::id()).rotate_left(17)
}

fn score_command(args: &[String]) -> Result<(), KeiriError> {
    if args.len() != 2 {
        return Err(KeiriError::ParseError(
            "score expects <category> <dice>".to_string(),
        ));
    }
    let category = parse_category(&args[0])?;
    let dice = Dice::parse(&args[1])?;
    let result = Rules::score(category, dice, &ScoreSheet::new());
    println!("{}", result.base_score);
    Ok(())
}

fn actions_command(args: &[String]) -> Result<(), KeiriError> {
    let state = parse_state(args)?;
    for action in Rules::legal_actions(&state) {
        println!("{action}");
    }
    Ok(())
}

fn advise_command(args: &[String]) -> Result<(), KeiriError> {
    let state = parse_state(args)?;
    let mut agent = OptimalAgent::new();
    match agent.best_action(&state) {
        Some(decision) => {
            println!("action: {}", decision.action);
            println!("expected_value: {:.3}", decision.expected_value);
            println!("confidence: {:.3}", agent.confidence(&state));
            println!("rationale: {}", agent.explain(&state));
            println!("cache_states: {}", agent.cache_len());
        }
        None => println!("terminal state; no action available"),
    }
    Ok(())
}

fn bbg_advise_command(args: &[String]) -> Result<(), KeiriError> {
    let mut oracle_endgame = 2;
    let mut agent = CliAgent::Auto;
    let mut table = None;
    let mut alternatives = 5usize;
    let mut snapshot_tokens = Vec::new();

    for arg in args {
        if let Some(value) = arg.strip_prefix("oracle_endgame=") {
            oracle_endgame = value
                .parse::<usize>()
                .map_err(|_| KeiriError::ParseError(format!("invalid oracle_endgame `{value}`")))?;
        } else if let Some(value) = arg.strip_prefix("agent=") {
            agent = parse_agent(value)?;
        } else if let Some(value) = arg.strip_prefix("table=") {
            table = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("alternatives=") {
            alternatives = value
                .parse::<usize>()
                .map_err(|_| KeiriError::ParseError(format!("invalid alternatives `{value}`")))?;
        } else {
            snapshot_tokens.push(arg.as_str());
        }
    }

    let snapshot = BuddyBoardGamesSnapshot::parse_compact_tokens(snapshot_tokens)?;
    let advice = match agent {
        CliAgent::Auto => {
            let table_path =
                table.unwrap_or_else(|| default_anchor_table_path(Ruleset::BuddyBoardGames));
            let table = ensure_anchor_table(&table_path, Ruleset::BuddyBoardGames)?;
            advise_buddyboardgames_snapshot_exact(&snapshot, table, alternatives)?
        }
        CliAgent::ExactTable => {
            let table_path =
                table.unwrap_or_else(|| default_anchor_table_path(Ruleset::BuddyBoardGames));
            let table = load_anchor_table_or_build_error(&table_path, Ruleset::BuddyBoardGames)?;
            advise_buddyboardgames_snapshot_exact(&snapshot, table, alternatives)?
        }
        CliAgent::Hybrid | CliAgent::Heuristic => {
            let hybrid_endgame = if agent == CliAgent::Heuristic {
                0
            } else {
                oracle_endgame
            };
            advise_buddyboardgames_snapshot(&snapshot, hybrid_endgame)?
        }
    };
    println!("{}", advice.to_cli_lines());
    Ok(())
}

fn build_table_command(args: &[String]) -> Result<(), KeiriError> {
    let mut out = None;
    let mut depths = vec![1, 2, 3];
    let mut state_tokens = Vec::new();

    for arg in args {
        if let Some(value) = arg.strip_prefix("out=") {
            out = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("depths=") {
            depths = parse_depths(value)?;
        } else {
            state_tokens.push(arg.as_str());
        }
    }

    let out =
        out.ok_or_else(|| KeiriError::ParseError("build-table expects out=<path>".to_string()))?;
    let state = GameState::parse_compact_tokens(state_tokens)?;
    let table = OracleTable::build_endgame(state.sheet().clone(), &depths)?;
    write_table(&out, &table)?;
    println!(
        "wrote {} rows to {out}; oracle cache states: {}",
        table.rows().len(),
        table.cache_states()
    );
    Ok(())
}

fn build_anchor_table_command(args: &[String]) -> Result<(), KeiriError> {
    let mut ruleset = Ruleset::BuddyBoardGames;
    let mut out = None;
    let mut max_open = Category::ALL.len();
    let mut build_options = AnchorBuildOptions::default();

    for arg in args {
        let (key, value) = arg.split_once('=').ok_or_else(|| {
            KeiriError::ParseError(format!(
                "build-anchor-table option `{arg}` must be key=value"
            ))
        })?;
        match key {
            "rules" => {
                ruleset = Ruleset::from_name(value)
                    .ok_or_else(|| KeiriError::ParseError(format!("unknown ruleset `{value}`")))?;
            }
            "out" => out = Some(value.to_string()),
            "max_open" => {
                max_open = value
                    .parse::<usize>()
                    .map_err(|_| KeiriError::ParseError(format!("invalid max_open `{value}`")))?;
            }
            "threads" => build_options.threads = parse_threads(value)?,
            "builder" => build_options.strategy = parse_anchor_builder(value)?,
            other => {
                return Err(KeiriError::ParseError(format!(
                    "unknown build-anchor-table option `{other}`"
                )));
            }
        }
    }

    let out = out.unwrap_or_else(|| default_anchor_table_path(ruleset));
    let table = build_anchor_table_resumable(ruleset, max_open, &out, build_options)?;
    if max_open == Category::ALL.len() {
        verify_anchor_table(&table)?;
    }
    save_anchor_table_atomically(&table, &out)?;
    if max_open == Category::ALL.len() {
        remove_partial_anchor_table(&out)?;
    }
    println!("wrote exact anchor table: {out}");
    println!("rules: {}", table.ruleset());
    println!("max_open: {max_open}");
    if max_open == Category::ALL.len() {
        println!(
            "initial_expected_value: {:.6}",
            table.expected_value(&GameState::new())?
        );
    }
    Ok(())
}

fn parse_state(args: &[String]) -> Result<GameState, KeiriError> {
    GameState::parse_compact_tokens(args.iter().map(String::as_str))
}

fn parse_category(input: &str) -> Result<Category, KeiriError> {
    Category::from_name(input)
        .ok_or_else(|| KeiriError::ParseError(format!("unknown category `{input}`")))
}

fn parse_depths(input: &str) -> Result<Vec<u8>, KeiriError> {
    let mut depths = Vec::new();
    for part in input.split(',') {
        let depth = part
            .parse::<u8>()
            .map_err(|_| KeiriError::ParseError(format!("invalid depth `{part}`")))?;
        if !(1..=3).contains(&depth) {
            return Err(KeiriError::InvalidRollCount(depth));
        }
        if !depths.contains(&depth) {
            depths.push(depth);
        }
    }
    if depths.is_empty() {
        return Err(KeiriError::ParseError(
            "depths must include at least one value".to_string(),
        ));
    }
    Ok(depths)
}

fn parse_bool_option(name: &str, value: &str) -> Result<bool, KeiriError> {
    match value {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(KeiriError::ParseError(format!(
            "invalid {name} boolean `{value}`"
        ))),
    }
}

fn parse_threads(value: &str) -> Result<Option<usize>, KeiriError> {
    if value == "auto" {
        return Ok(None);
    }
    let threads = value
        .parse::<usize>()
        .map_err(|_| KeiriError::ParseError(format!("invalid threads `{value}`")))?;
    if threads == 0 {
        return Err(KeiriError::ParseError(
            "threads must be `auto` or a positive integer".to_string(),
        ));
    }
    Ok(Some(threads))
}

fn parse_anchor_builder(value: &str) -> Result<AnchorBuildStrategy, KeiriError> {
    match value {
        "dense" => Ok(AnchorBuildStrategy::Dense),
        "recursive" => Ok(AnchorBuildStrategy::Recursive),
        _ => Err(KeiriError::ParseError(format!(
            "unknown anchor table builder `{value}`"
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliAgent {
    Auto,
    ExactTable,
    Hybrid,
    Heuristic,
}

impl CliAgent {
    fn as_cli_value(self) -> &'static str {
        match self {
            CliAgent::Auto => "auto",
            CliAgent::ExactTable => "exact-table",
            CliAgent::Hybrid => "hybrid",
            CliAgent::Heuristic => "heuristic",
        }
    }
}

fn parse_agent(value: &str) -> Result<CliAgent, KeiriError> {
    match value {
        "auto" | "best" | "smartest" => Ok(CliAgent::Auto),
        "exact-table" | "exact" | "table" => Ok(CliAgent::ExactTable),
        "hybrid" | "hybrid-v2" => Ok(CliAgent::Hybrid),
        "heuristic" => Ok(CliAgent::Heuristic),
        _ => Err(KeiriError::ParseError(format!("unknown agent `{value}`"))),
    }
}

fn default_agent_for_ruleset(ruleset: Ruleset) -> CliAgent {
    match ruleset {
        Ruleset::HasbroStrict => CliAgent::Auto,
        Ruleset::BuddyBoardGames => CliAgent::Auto,
    }
}

fn default_anchor_table_path(ruleset: Ruleset) -> String {
    match ruleset {
        Ruleset::HasbroStrict => "target/keiri_tables/hasbro-anchor-v1.bin".to_string(),
        Ruleset::BuddyBoardGames => "target/keiri_tables/bbg-anchor-v1.bin".to_string(),
    }
}

fn load_anchor_table_or_build_error(
    path: &str,
    expected_ruleset: Ruleset,
) -> Result<AnchorValueTable, KeiriError> {
    let table = AnchorValueTable::load(path).map_err(|error| {
        KeiriError::InvalidAnchorTable(format!(
            "{error}. Build it with: keiri build-anchor-table rules={expected_ruleset} out={path}"
        ))
    })?;
    if table.ruleset() != expected_ruleset {
        return Err(KeiriError::InvalidAnchorTable(format!(
            "anchor table `{path}` is for {}, expected {}",
            table.ruleset(),
            expected_ruleset
        )));
    }
    Ok(table)
}

fn ensure_anchor_table(path: &str, ruleset: Ruleset) -> Result<AnchorValueTable, KeiriError> {
    if Path::new(path).exists() {
        match load_anchor_table_or_build_error(path, ruleset) {
            Ok(table) => {
                verify_anchor_table(&table)?;
                return Ok(table);
            }
            Err(error) => {
                println!(
                    "Existing exact {ruleset} anchor table at {path} is not reusable: {error}"
                );
            }
        }
    }

    build_missing_anchor_table(path, ruleset)?;
    let table = load_anchor_table_or_build_error(path, ruleset)?;
    verify_anchor_table(&table)?;
    println!("Exact {ruleset} anchor table verified: {path}");
    Ok(table)
}

fn build_missing_anchor_table(path: &str, ruleset: Ruleset) -> Result<(), KeiriError> {
    println!(
        "Exact {ruleset} anchor table not found; building it now at {path}. This can take a while on the first run."
    );

    if cfg!(debug_assertions) && env::var_os("KEIRI_BUILDING_ANCHOR_TABLE").is_none() {
        println!("Using the optimized release builder for table generation.");
        let status = process::Command::new("cargo")
            .arg("run")
            .arg("--release")
            .arg("--")
            .arg("build-anchor-table")
            .arg(format!("rules={ruleset}"))
            .arg(format!("out={path}"))
            .env("KEIRI_BUILDING_ANCHOR_TABLE", "1")
            .status()
            .map_err(|error| {
                KeiriError::ParseError(format!("failed to launch release table builder: {error}"))
            })?;
        if !status.success() {
            return Err(KeiriError::ParseError(format!(
                "release table builder exited with {status}"
            )));
        }
        return Ok(());
    }

    let table = build_anchor_table_resumable(
        ruleset,
        Category::ALL.len(),
        path,
        AnchorBuildOptions::default(),
    )?;
    save_anchor_table_atomically(&table, path)?;
    remove_partial_anchor_table(path)
}

fn build_anchor_table_resumable(
    ruleset: Ruleset,
    max_open: usize,
    final_path: &str,
    build_options: AnchorBuildOptions,
) -> Result<AnchorValueTable, KeiriError> {
    let started = Instant::now();
    let partial_path = format!("{final_path}.partial");
    let table = if Path::new(&partial_path).exists() {
        match load_anchor_table_or_build_error(&partial_path, ruleset) {
            Ok(partial) => {
                let completed = partial.completed_open_layers();
                println!(
                    "Resuming exact {ruleset} table from {partial_path}; completed layers: {}",
                    completed
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                );
                partial
            }
            Err(error) => {
                println!(
                    "Ignoring incompatible partial table {partial_path}: {error}; starting a fresh build."
                );
                AnchorValueTable::build_limited_with_options(ruleset, 0, build_options)?
            }
        }
    } else {
        AnchorValueTable::build_limited_with_options(ruleset, 0, build_options)?
    };

    AnchorValueTable::build_from_partial_with_options_and_callbacks(
        table,
        max_open,
        build_options,
        |progress| print_anchor_progress(progress, started),
        |open_count, table| {
            save_anchor_table_atomically(table, &partial_path)?;
            println!("checkpointed exact {ruleset} table after layer {open_count}");
            Ok(())
        },
    )
}

fn print_anchor_progress(progress: AnchorBuildProgress, started: Instant) {
    if progress.layer_states == 0 {
        println!(
            "anchor layer {}/{}: no valid states ({:.1}s elapsed)",
            progress.open_count,
            progress.total_open_count,
            started.elapsed().as_secs_f64()
        );
        return;
    }
    let percent = progress.completed_layer_states as f64 * 100.0 / progress.layer_states as f64;
    println!(
        "anchor layer {}/{}: {}/{} states ({percent:.1}%, {:.1}s elapsed)",
        progress.open_count,
        progress.total_open_count,
        progress.completed_layer_states,
        progress.layer_states,
        started.elapsed().as_secs_f64()
    );
}

fn save_anchor_table_atomically(table: &AnchorValueTable, path: &str) -> Result<(), KeiriError> {
    let temp = format!("{path}.tmp");
    let _ = fs::remove_file(&temp);
    table.save(&temp)?;
    fs::rename(&temp, path).map_err(|error| {
        KeiriError::ParseError(format!("failed to move {temp} to {path}: {error}"))
    })
}

fn remove_partial_anchor_table(path: &str) -> Result<(), KeiriError> {
    let partial = format!("{path}.partial");
    match fs::remove_file(&partial) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(KeiriError::ParseError(format!(
            "failed to remove partial table {partial}: {error}"
        ))),
    }
}

fn verify_anchor_table(table: &AnchorValueTable) -> Result<(), KeiriError> {
    let initial = GameState::new();
    let value = table.expected_value(&initial)?;
    if !value.is_finite() || value <= 0.0 {
        return Err(KeiriError::InvalidAnchorTable(format!(
            "anchor table initial expected value is invalid: {value}"
        )));
    }
    if table.ruleset() == Ruleset::BuddyBoardGames
        && (value - BBG_OPENING_EXPECTED_VALUE).abs() > OPENING_EXPECTED_VALUE_TOLERANCE
    {
        return Err(KeiriError::InvalidAnchorTable(format!(
            "BuddyBoardGames anchor table opening expected value {value:.6} does not match target {BBG_OPENING_EXPECTED_VALUE:.6} within {OPENING_EXPECTED_VALUE_TOLERANCE:.6}; rebuild the table with current rules"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct EvaluationSummary {
    timestamp_unix: u64,
    agent: String,
    ruleset: Ruleset,
    games: usize,
    seed: u64,
    oracle_endgame: usize,
    scores: Vec<u16>,
    upper_bonus_count: usize,
    yahtzee_bonus_games: usize,
}

impl EvaluationSummary {
    fn mean(&self) -> f64 {
        self.scores
            .iter()
            .map(|score| f64::from(*score))
            .sum::<f64>()
            / self.scores.len() as f64
    }

    fn min(&self) -> u16 {
        *self.scores.iter().min().expect("evaluation has scores")
    }

    fn max(&self) -> u16 {
        *self.scores.iter().max().expect("evaluation has scores")
    }

    fn percentile(&self, percentile: f64) -> u16 {
        let mut scores = self.scores.clone();
        scores.sort_unstable();
        let index = ((scores.len() - 1) as f64 * percentile).round() as usize;
        scores[index]
    }

    fn upper_bonus_rate(&self) -> f64 {
        self.upper_bonus_count as f64 / self.games as f64
    }

    fn yahtzee_bonus_rate(&self) -> f64 {
        self.yahtzee_bonus_games as f64 / self.games as f64
    }

    fn to_cli_lines(&self) -> String {
        [
            format!("agent: {}", self.agent),
            format!("rules: {}", self.ruleset),
            format!("games: {}", self.games),
            format!("seed: {}", self.seed),
            format!("oracle_endgame: {}", self.oracle_endgame),
            format!("mean: {:.3}", self.mean()),
            format!("min: {}", self.min()),
            format!("p05: {}", self.percentile(0.05)),
            format!("p50: {}", self.percentile(0.50)),
            format!("p95: {}", self.percentile(0.95)),
            format!("max: {}", self.max()),
            format!("upper_bonus_rate: {:.4}", self.upper_bonus_rate()),
            format!("yahtzee_bonus_rate: {:.4}", self.yahtzee_bonus_rate()),
        ]
        .join("\n")
    }

    fn csv_header() -> &'static str {
        "timestamp_unix,agent,rules,games,seed,oracle_endgame,mean,min,p05,p50,p95,max,upper_bonus_rate,yahtzee_bonus_rate\n"
    }

    fn csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{},{:.6},{},{},{},{},{},{:.6},{:.6}\n",
            self.timestamp_unix,
            self.agent,
            self.ruleset,
            self.games,
            self.seed,
            self.oracle_endgame,
            self.mean(),
            self.min(),
            self.percentile(0.05),
            self.percentile(0.50),
            self.percentile(0.95),
            self.max(),
            self.upper_bonus_rate(),
            self.yahtzee_bonus_rate()
        )
    }
}

fn run_evaluation(
    games: usize,
    seed: u64,
    ruleset: Ruleset,
    oracle_endgame: usize,
    agent: CliAgent,
    table_path: Option<&str>,
) -> Result<EvaluationSummary, KeiriError> {
    let timestamp_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut seed_rng = keiri::Rng64::new(seed);
    let mut scores = Vec::with_capacity(games);
    let mut upper_bonus_count = 0;
    let mut yahtzee_bonus_games = 0;

    match agent {
        CliAgent::Auto => {
            let path = table_path
                .map(str::to_string)
                .unwrap_or_else(|| default_anchor_table_path(ruleset));
            let table = ensure_anchor_table(&path, ruleset)?;
            let mut exact_agent = ExactTableAgent::new(table);
            for _ in 0..games {
                let game_seed = seed_rng.next_u64();
                let report = simulate_with_agent(game_seed, ruleset, &mut exact_agent, false)?;
                scores.push(report.final_score);
                upper_bonus_count += usize::from(report.upper_bonus);
                yahtzee_bonus_games += usize::from(report.yahtzee_bonus_count > 0);
            }
        }
        CliAgent::ExactTable => {
            let path = table_path
                .map(str::to_string)
                .unwrap_or_else(|| default_anchor_table_path(ruleset));
            let table = load_anchor_table_or_build_error(&path, ruleset)?;
            verify_anchor_table(&table)?;
            let mut exact_agent = ExactTableAgent::new(table);
            for _ in 0..games {
                let game_seed = seed_rng.next_u64();
                let report = simulate_with_agent(game_seed, ruleset, &mut exact_agent, false)?;
                scores.push(report.final_score);
                upper_bonus_count += usize::from(report.upper_bonus);
                yahtzee_bonus_games += usize::from(report.yahtzee_bonus_count > 0);
            }
        }
        CliAgent::Hybrid | CliAgent::Heuristic => {
            let hybrid_endgame = if agent == CliAgent::Heuristic {
                0
            } else {
                oracle_endgame
            };
            for _ in 0..games {
                let game_seed = seed_rng.next_u64();
                let mut simulator = GameSimulator::new(game_seed, ruleset, hybrid_endgame);
                let report = simulator.simulate(false)?;
                scores.push(report.final_score);
                upper_bonus_count += usize::from(report.upper_bonus);
                yahtzee_bonus_games += usize::from(report.yahtzee_bonus_count > 0);
            }
        }
    }

    Ok(EvaluationSummary {
        timestamp_unix,
        agent: agent.as_cli_value().to_string(),
        ruleset,
        games,
        seed,
        oracle_endgame,
        scores,
        upper_bonus_count,
        yahtzee_bonus_games,
    })
}

fn simulate_report(
    seed: u64,
    ruleset: Ruleset,
    oracle_endgame: usize,
    agent: CliAgent,
    table_path: Option<&str>,
    verbose: bool,
) -> Result<keiri::SimulationReport, KeiriError> {
    match agent {
        CliAgent::Auto => {
            let path = table_path
                .map(str::to_string)
                .unwrap_or_else(|| default_anchor_table_path(ruleset));
            let table = ensure_anchor_table(&path, ruleset)?;
            let mut exact_agent = ExactTableAgent::new(table);
            simulate_with_agent(seed, ruleset, &mut exact_agent, verbose)
        }
        CliAgent::ExactTable => {
            let path = table_path
                .map(str::to_string)
                .unwrap_or_else(|| default_anchor_table_path(ruleset));
            let table = load_anchor_table_or_build_error(&path, ruleset)?;
            verify_anchor_table(&table)?;
            let mut exact_agent = ExactTableAgent::new(table);
            simulate_with_agent(seed, ruleset, &mut exact_agent, verbose)
        }
        CliAgent::Hybrid | CliAgent::Heuristic => {
            let hybrid_endgame = if agent == CliAgent::Heuristic {
                0
            } else {
                oracle_endgame
            };
            let mut simulator = GameSimulator::new(seed, ruleset, hybrid_endgame);
            simulator.simulate(verbose)
        }
    }
}

fn write_evaluation_summary(
    path: &str,
    summary: &EvaluationSummary,
    append: bool,
) -> Result<(), KeiriError> {
    let path = Path::new(path);
    ensure_parent_dir(path)?;
    let needs_header =
        !append || !path.exists() || path.metadata().map_or(0, |meta| meta.len()) == 0;
    let mut output = String::new();
    if needs_header {
        output.push_str(EvaluationSummary::csv_header());
    }
    output.push_str(&summary.csv_row());

    if append {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                KeiriError::ParseError(format!("failed to open {}: {error}", path.display()))
            })?;
        file.write_all(output.as_bytes()).map_err(|error| {
            KeiriError::ParseError(format!("failed to write {}: {error}", path.display()))
        })?;
    } else {
        fs::write(path, output).map_err(|error| {
            KeiriError::ParseError(format!("failed to write {}: {error}", path.display()))
        })?;
    }
    Ok(())
}

fn write_score_series(path: &str, summary: &EvaluationSummary) -> Result<(), KeiriError> {
    let path = Path::new(path);
    ensure_parent_dir(path)?;
    let mut output = String::from("game,seed,score\n");
    let mut seed_rng = keiri::Rng64::new(summary.seed);
    for (index, score) in summary.scores.iter().enumerate() {
        output.push_str(&format!(
            "{},{},{}\n",
            index + 1,
            seed_rng.next_u64(),
            score
        ));
    }
    fs::write(path, output).map_err(|error| {
        KeiriError::ParseError(format!("failed to write {}: {error}", path.display()))
    })
}

fn write_table(path: &str, table: &OracleTable) -> Result<(), KeiriError> {
    let path = Path::new(path);
    ensure_parent_dir(path)?;
    fs::write(path, table.to_tsv()).map_err(|error| {
        KeiriError::ParseError(format!("failed to write {}: {error}", path.display()))
    })
}

fn ensure_parent_dir(path: &Path) -> Result<(), KeiriError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            KeiriError::ParseError(format!("failed to create {}: {error}", parent.display()))
        })?;
    }
    Ok(())
}

fn print_usage() {
    println!(
        "Keiri Yahtzee oracle

USAGE:
  keiri simulate [seed=<u64>] [verbose=true] [rules=hasbro|buddyboardgames] [agent=auto|hybrid|heuristic|exact-table] [table=<path>] [oracle_endgame=2]
  keiri --simulate [seed=<u64>] [verbose=true] [rules=hasbro|buddyboardgames] [agent=auto|hybrid|heuristic|exact-table] [table=<path>] [oracle_endgame=2]
  keiri --bbg-join <room-code> [player=keiri-bot] [play=true] [start=false]
  keiri bbg-join [room=<room-code>] [player=keiri-bot] [play=true] [start=false]
  keiri evaluate [games=100] [seed=1] [rules=hasbro|buddyboardgames] [agent=auto|hybrid|heuristic|exact-table] [table=<path>] [oracle_endgame=0] [out=metrics/simulation_history.csv] [scores_out=<path>]
  keiri score <category> <dice>
  keiri actions <state>
  keiri advise <state>
  keiri bbg-advise [agent=auto|exact-table|heuristic] [table=<path>] <buddyboardgames-snapshot>
  keiri build-table out=<path> depths=1,2,3 <score-sheet-state>
  keiri build-anchor-table rules=buddyboardgames out=target/keiri_tables/bbg-anchor-v1.bin [threads=auto|n] [builder=dense|recursive]

EXAMPLES:
  keiri simulate
  keiri --simulate seed=42 verbose=true
  keiri simulate rules=buddyboardgames agent=auto table=target/keiri_tables/bbg-anchor-v1.bin
  keiri --bbg-join my-room-code
  keiri bbg-join room=my-room-code player=keiri-bot play=true
  keiri evaluate games=1000 seed=1 out=metrics/simulation_history.csv scores_out=metrics/scores.csv
  keiri evaluate rules=buddyboardgames games=1000 seed=1 agent=auto table=target/keiri_tables/bbg-anchor-v1.bin out=none
  keiri score full-house 2,2,3,3,3
  keiri actions dice=1,2,3,4,5 rolls=2 scores=ones:3,twos:6
  keiri advise dice=6,6,6,6,6 rolls=3 scores=ones:3,twos:6,threes:9,fours:12,fives:15,sixes:18,three-kind:24,four-kind:24,full-house:25,small-straight:30,large-straight:40,yahtzee:50
  keiri bbg-advise state=STARTED me=0 turn=0 spectator=false pending=false dice=1,2,3,4,5 selected=0,0,0,0,0 rolls=2 rows=0:3:1,1:6:1
  keiri build-table out=target/keiri_tables/chance.tsv depths=2,3 scores=ones:0,twos:0,threes:0,fours:0,fives:0,sixes:0,three-kind:0,four-kind:0,full-house:0,small-straight:0,large-straight:0,yahtzee:50
  keiri build-anchor-table rules=hasbro out=target/keiri_tables/hasbro-anchor-v1.bin threads=auto builder=dense

STATE FORMAT:
  dice=1,2,3,4,5 or dice=none
  rolls=0..3
  scores=category:score,category:score
  yahtzee_bonus=<count>"
    );
}
