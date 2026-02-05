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
    pub const CLOSED_THREE: i32 = 1_000;

    // Building patterns
    /// Open two: _OO_ (potential to grow)
    pub const OPEN_TWO: i32 = 500;
    /// Closed two: XOO_ or _OOX (one side blocked)
    pub const CLOSED_TWO: i32 = 50;

    // Capture related
    /// Can capture opponent's pair next move
    pub const CAPTURE_THREAT: i32 = 3_000;
    /// Value per captured pair
    pub const CAPTURE_PAIR: i32 = 500;
    /// 4 pairs captured (one more = win)
    pub const NEAR_CAPTURE_WIN: i32 = 8_000;

    // Defense weights
    /// Defense is weighted higher than offense
    pub const DEFENSE_MULTIPLIER: f32 = 1.5;
}

/// Capture-based scoring with non-linear weights
///
/// The scoring is exponential as captures approach the winning threshold.
/// Defense is weighted higher to ensure the AI responds to capture threats.
///
/// # Arguments
/// * `my_captures` - Number of pairs captured by the player
/// * `opp_captures` - Number of pairs captured by the opponent
///
/// # Returns
/// Score differential (positive = advantage, negative = disadvantage)
pub fn capture_score(my_captures: u8, opp_captures: u8) -> i32 {
    // Non-linear scoring - closer to win = exponentially more valuable
    const CAP_WEIGHTS: [i32; 6] = [0, 200, 600, 2000, 8000, PatternScore::CAPTURE_WIN];

    let my_score = CAP_WEIGHTS[my_captures.min(5) as usize];
    let opp_score = CAP_WEIGHTS[opp_captures.min(5) as usize];

    my_score - (opp_score as f32 * PatternScore::DEFENSE_MULTIPLIER) as i32
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
        assert!(score >= 8000, "4 captures should be highly valuable");
    }

    #[test]
    fn test_capture_score_defense_weighted() {
        let own_captures = capture_score(1, 0);
        let opp_captures = capture_score(0, 1);
        // Defense should be weighted more (negative score should be larger in magnitude)
        assert!(
            own_captures < -opp_captures,
            "Defense should be weighted higher: own={}, opp={}",
            own_captures,
            opp_captures
        );
    }

    #[test]
    fn test_capture_score_win() {
        let score = capture_score(5, 0);
        assert_eq!(score, PatternScore::CAPTURE_WIN);
    }

    #[test]
    fn test_capture_score_symmetry_with_defense_weight() {
        // Opponent having captures should hurt more than our captures help
        let we_lead = capture_score(2, 1);
        let they_lead = capture_score(1, 2);

        // Both should reflect the asymmetry
        assert!(we_lead > 0, "Leading in captures should be positive");
        assert!(they_lead < 0, "Trailing in captures should be negative");
        assert!(
            we_lead.abs() < they_lead.abs(),
            "Defense weight should make trailing worse than leading is good"
        );
    }
}
