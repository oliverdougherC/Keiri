use std::time::Instant;

use keiri::{Category, Dice, GameState, OptimalAgent, ScoreSheet};

fn main() {
    let mut sheet = ScoreSheet::new();
    for category in Category::ALL {
        if category != Category::Chance {
            let score = if category == Category::Yahtzee { 50 } else { 0 };
            sheet.fill_raw(category, score).unwrap();
        }
    }
    let state = GameState::from_parts(Some(Dice::new([1, 2, 3, 4, 6]).unwrap()), 2, sheet).unwrap();
    let mut agent = OptimalAgent::new();

    let started = Instant::now();
    let value = agent.expected_value(&state);
    println!(
        "single-category expected value: {value:.3}; states: {}; elapsed: {:?}",
        agent.cache_len(),
        started.elapsed()
    );
}
