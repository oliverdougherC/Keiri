# Keiri Ruleset

Keiri v1 uses US Hasbro-style solitaire Yahtzee as its canonical ruleset.

## Categories

The score sheet has 13 categories:

- Ones, twos, threes, fours, fives, sixes
- Three of a kind
- Four of a kind
- Full house
- Small straight
- Large straight
- Yahtzee
- Chance

The upper section earns a 35-point bonus when the upper subtotal reaches at
least 63.

## Scoring

Upper categories score the sum of dice matching the category face. Three and
four of a kind score the sum of all dice when the required count is present.
Full house scores 25 for a three-plus-two pattern. Small straight scores 30 for
any four-die run, and large straight scores 40 for `1,2,3,4,5` or `2,3,4,5,6`.
Yahtzee scores 50 for five matching dice. Chance scores the sum of all dice.

A natural Yahtzee does not count as a normal full house unless Joker rules are
active.

## Yahtzee Bonus and Joker Rules

An additional Yahtzee earns a 100-point bonus only when the Yahtzee category is
already filled with 50.

When that bonus condition is active, Joker rules also apply:

1. If the matching upper category is open, it must be scored.
2. If the matching upper category is filled, any open lower category may be
   scored.
3. If all lower categories are filled, any open upper category may be scored.

Under Joker rules, full house, small straight, and large straight receive their
fixed scores even though all dice show the same face.

If the Yahtzee category was filled with zero, additional Yahtzees are scored
normally and do not activate the bonus or Joker rules.
