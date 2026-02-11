//! Pattern scores for Gomoku evaluation
//!
//! These constants define the scoring weights for various board patterns.
//! Carefully tuned for strong play with Ninuki-renju rules.

/// Pattern scores for evaluation
/// These are carefully tuned for strong play
pub struct PatternScore;

impl PatternScore {
    // Winning patterns
    /// Five in a row - immediate win
    pub const FIVE: i32 = 1_000_000;
    /// Capture win (5 pairs captured)
    pub const CAPTURE_WIN: i32 = 1_000_000;

    // Strong attacking patterns
    /// Open four: _OOOO_ (unstoppable without capture)
    pub const OPEN_FOUR: i32 = 100_000;
    /// Closed four: XOOOO_ or _OOOOX (one way to extend)
    pub const CLOSED_FOUR: i32 = 50_000;

    // Moderate threats
    /// Open three: _OOO_ (becomes open four if not blocked)
    pub const OPEN_THREE: i32 = 10_000;
    /// Closed three: XOOO_ or _OOOX (one side blocked)
    /// Must be well below OPEN_THREE (10K) — a closed three is half as dangerous
    /// since the opponent has a clear blocking point.
    pub const CLOSED_THREE: i32 = 1_500;

    // Building patterns
    /// Open two: _OO_ (potential to grow)
    pub const OPEN_TWO: i32 = 1_000;
    /// Closed two: XOO_ or _OOX (one side blocked)
    pub const CLOSED_TWO: i32 = 200;

    // Capture related — Pente captures are critical in Ninuki-renju.
    // A single capture removes 2 opponent stones AND advances toward capture win.
    /// Can capture opponent's pair next move
    pub const CAPTURE_THREAT: i32 = 8_000;
    /// Value per captured pair
    pub const CAPTURE_PAIR: i32 = 2_000;
    /// 4 pairs captured (one more = win) - must be >> OPEN_FOUR
    pub const NEAR_CAPTURE_WIN: i32 = 80_000;

    // Note: Defense-first behavior is handled by move ordering (score_move),
    // NOT by the evaluation function. The evaluation must be symmetric
    // for negamax correctness: evaluate(board, A) == -evaluate(board, B).
}

/// Capture-based scoring with non-linear weights
///
/// The scoring is exponential as captures approach the winning threshold.
/// MUST be symmetric for negamax: capture_score(a, b) == -capture_score(b, a).
///
/// # Arguments
/// * `my_captures` - Number of pairs captured by the player
/// * `opp_captures` - Number of pairs captured by the opponent
///
/// # Returns
/// Score differential (positive = advantage, negative = disadvantage)
pub fn capture_score(my_captures: u8, opp_captures: u8) -> i32 {
    // Non-linear scoring - closer to win = exponentially more valuable
    // Each level must be significantly higher than pattern threats at that stage
    // to ensure the AI treats capture accumulation as a serious strategic factor.
    const CAP_WEIGHTS: [i32; 6] = [
        0,
        2_000,     // 1 capture: minor advantage
        7_000,     // 2 captures: moderate (> CLOSED_THREE)
        20_000,    // 3 captures: serious threat (> OPEN_THREE)
        PatternScore::NEAR_CAPTURE_WIN, // 4 captures: 80K, near-winning
        PatternScore::CAPTURE_WIN,      // 5 captures: 1M, game over
    ];

    let my_score = CAP_WEIGHTS[my_captures.min(5) as usize];
    let opp_score = CAP_WEIGHTS[opp_captures.min(5) as usize];

    my_score - opp_score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_score_hierarchy() {
        // Verify score hierarchy makes sense
        assert!(PatternScore::FIVE > PatternScore::OPEN_FOUR);
        assert!(PatternScore::OPEN_FOUR > PatternScore::CLOSED_FOUR);
        assert!(PatternScore::CLOSED_FOUR > PatternScore::OPEN_THREE);
        assert!(PatternScore::OPEN_THREE > PatternScore::CLOSED_THREE);
        assert!(PatternScore::CLOSED_THREE > PatternScore::OPEN_TWO);
        assert!(PatternScore::OPEN_TWO > PatternScore::CLOSED_TWO);
    }

    #[test]
    fn test_capture_score_zero() {
        assert_eq!(capture_score(0, 0), 0);
    }

    #[test]
    fn test_capture_score_advantage() {
        let score = capture_score(2, 0);
        assert!(score > 0, "Should be positive for capture advantage");
    }

    #[test]
    fn test_capture_score_near_win() {
        let score = capture_score(4, 0);
        assert!(score >= 60_000, "4 captures should be highly valuable (near-win)");
    }

    #[test]
    fn test_capture_score_symmetric() {
        // Negamax requires: capture_score(a, b) == -capture_score(b, a)
        let score_1_0 = capture_score(1, 0);
        let score_0_1 = capture_score(0, 1);
        assert_eq!(
            score_1_0, -score_0_1,
            "capture_score must be symmetric: (1,0)={}, (0,1)={}",
            score_1_0, score_0_1
        );

        let score_2_1 = capture_score(2, 1);
        let score_1_2 = capture_score(1, 2);
        assert_eq!(
            score_2_1, -score_1_2,
            "capture_score must be symmetric: (2,1)={}, (1,2)={}",
            score_2_1, score_1_2
        );
    }

    #[test]
    fn test_capture_score_win() {
        let score = capture_score(5, 0);
        assert_eq!(score, PatternScore::CAPTURE_WIN);
    }

    #[test]
    fn test_capture_score_negamax_symmetry() {
        // Verify negamax property: score(a,b) == -score(b,a) for all values
        for a in 0..=5u8 {
            for b in 0..=5u8 {
                let score_ab = capture_score(a, b);
                let score_ba = capture_score(b, a);
                assert_eq!(
                    score_ab, -score_ba,
                    "Negamax symmetry violated: capture_score({},{})={}, capture_score({},{})={}",
                    a, b, score_ab, b, a, score_ba
                );
            }
        }
    }
}
