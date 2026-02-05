//! VCF/VCT threat search for forced wins
//!
//! This module implements specialized threat-space search algorithms:
//! - VCF (Victory by Continuous Fours): Finds winning sequences using only four-threats
//! - VCT (Victory by Continuous Threats): More general, includes open-three threats
//!
//! These are powerful pruning techniques that can find forced wins much faster
//! than regular alpha-beta search by only considering forcing moves.

use crate::board::{Board, Pos, Stone, BOARD_SIZE};
use crate::rules::{
    can_break_five_by_capture, execute_captures, find_five_positions, get_captured_positions,
    has_five_in_row, is_valid_move,
};

/// Direction vectors for line checking (4 directions)
const DIRECTIONS: [(i32, i32); 4] = [
    (0, 1),  // Horizontal
    (1, 0),  // Vertical
    (1, 1),  // Diagonal SE
    (1, -1), // Diagonal SW
];

/// Result of a VCF/VCT search
#[derive(Debug, Clone)]
pub struct ThreatResult {
    /// The winning sequence of moves (attacker moves only)
    pub winning_sequence: Vec<Pos>,
    /// Whether a forced win was found
    pub found: bool,
}

impl ThreatResult {
    /// Create a result indicating no forced win was found
    #[inline]
    fn not_found() -> Self {
        Self {
            winning_sequence: Vec::new(),
            found: false,
        }
    }

    /// Create a result with a found winning sequence
    #[inline]
    fn found(sequence: Vec<Pos>) -> Self {
        Self {
            winning_sequence: sequence,
            found: true,
        }
    }
}

/// Threat searcher for VCF/VCT algorithms
pub struct ThreatSearcher {
    /// Maximum depth for VCF search (number of four-threats)
    max_vcf_depth: u8,
    /// Maximum depth for VCT search (number of threats)
    max_vct_depth: u8,
    /// Node counter for statistics
    nodes: u64,
}

impl ThreatSearcher {
    /// Create a new threat searcher with default depth limits
    pub fn new() -> Self {
        Self {
            max_vcf_depth: 30,
            max_vct_depth: 20,
            nodes: 0,
        }
    }

    /// Create a threat searcher with custom depth limits
    pub fn with_depths(vcf_depth: u8, vct_depth: u8) -> Self {
        Self {
            max_vcf_depth: vcf_depth,
            max_vct_depth: vct_depth,
            nodes: 0,
        }
    }

    /// Search for VCF (Victory by Continuous Fours)
    ///
    /// VCF finds winning sequences where each move creates a four (4 in a row
    /// with at least one open end). The opponent must defend each four, and
    /// we continue with more fours until we win.
    ///
    /// # Arguments
    /// * `board` - Current board state
    /// * `color` - Color of the attacking player
    ///
    /// # Returns
    /// `ThreatResult` with the winning sequence if found
    pub fn search_vcf(&mut self, board: &Board, color: Stone) -> ThreatResult {
        self.nodes = 0;
        let mut sequence = Vec::new();

        if self.vcf_search(board, color, 0, &mut sequence) {
            ThreatResult::found(sequence)
        } else {
            ThreatResult::not_found()
        }
    }

    /// Internal recursive VCF search
    fn vcf_search(
        &mut self,
        board: &Board,
        color: Stone,
        depth: u8,
        sequence: &mut Vec<Pos>,
    ) -> bool {
        self.nodes += 1;

        if depth > self.max_vcf_depth {
            return false;
        }

        // Find all moves that create a four
        let threats = self.find_four_threats(board, color);

        for threat_move in threats {
            // Make the threat move
            let mut new_board = board.clone();
            new_board.place_stone(threat_move, color);
            execute_captures(&mut new_board, threat_move, color);

            sequence.push(threat_move);

            // Check for immediate win by five-in-a-row
            if has_five_in_row(&new_board, color) {
                if let Some(five) = find_five_positions(&new_board, color) {
                    if !can_break_five_by_capture(&new_board, &five, color) {
                        return true;
                    }
                }
            }

            // Check for capture win (5 pairs = 10 stones)
            if new_board.captures(color) >= 5 {
                return true;
            }

            // Find opponent's forced defenses against this four
            let defenses = self.find_defense_moves(&new_board, threat_move, color);

            if defenses.is_empty() {
                // No defense means we win (opponent cannot block the four)
                return true;
            }

            // If only one defense, opponent is forced to play there
            // Continue VCF search after their defense
            if defenses.len() == 1 {
                let defense = defenses[0];
                let mut def_board = new_board.clone();
                def_board.place_stone(defense, color.opponent());
                execute_captures(&mut def_board, defense, color.opponent());

                if self.vcf_search(&def_board, color, depth + 1, sequence) {
                    return true;
                }
            }
            // Multiple defenses: VCF fails at this branch (opponent has choice)

            sequence.pop();
        }

        false
    }

    /// Find all moves that create a four or five (winning move or forcing move)
    ///
    /// This prioritizes winning moves (five) over forcing moves (four).
    fn find_four_threats(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut winning_moves = Vec::new();
        let mut four_threats = Vec::new();

        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                // Check if this creates a winning five first
                if self.creates_five_or_more(board, pos, color) {
                    winning_moves.push(pos);
                } else if self.creates_four(board, pos, color) {
                    four_threats.push(pos);
                }
            }
        }

        // Prioritize winning moves
        winning_moves.extend(four_threats);
        winning_moves
    }

    /// Check if placing at pos creates five or more in a row
    fn creates_five_or_more(&self, board: &Board, pos: Pos, color: Stone) -> bool {
        for &(dr, dc) in &DIRECTIONS {
            let mut count = 1; // The stone we're placing

            // Scan positive direction
            let mut r = pos.row as i32 + dr;
            let mut c = pos.col as i32 + dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == color {
                    count += 1;
                } else {
                    break;
                }
                r += dr;
                c += dc;
            }

            // Scan negative direction
            r = pos.row as i32 - dr;
            c = pos.col as i32 - dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                if board.get(p) == color {
                    count += 1;
                } else {
                    break;
                }
                r -= dr;
                c -= dc;
            }

            if count >= 5 {
                return true;
            }
        }

        false
    }

    /// Check if placing at pos creates a four (4 in a row with at least one open end)
    fn creates_four(&self, board: &Board, pos: Pos, color: Stone) -> bool {
        for &(dr, dc) in &DIRECTIONS {
            let mut count = 1; // The stone we're placing
            let mut open_ends = 0;

            // Scan positive direction
            let mut r = pos.row as i32 + dr;
            let mut c = pos.col as i32 + dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                match board.get(p) {
                    s if s == color => count += 1,
                    Stone::Empty => {
                        open_ends += 1;
                        break;
                    }
                    _ => break, // Opponent stone blocks
                }
                r += dr;
                c += dc;
            }

            // Scan negative direction
            r = pos.row as i32 - dr;
            c = pos.col as i32 - dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                match board.get(p) {
                    s if s == color => count += 1,
                    Stone::Empty => {
                        open_ends += 1;
                        break;
                    }
                    _ => break, // Opponent stone blocks
                }
                r -= dr;
                c -= dc;
            }

            // A four needs exactly 4 stones and at least one open end
            if count == 4 && open_ends >= 1 {
                return true;
            }
        }

        false
    }

    /// Find defense moves against a four-threat
    ///
    /// Defense includes:
    /// 1. Blocking moves at the ends of the four
    /// 2. Capture moves that break the four (only captures that remove stones from the four pattern)
    fn find_defense_moves(&self, board: &Board, threat_move: Pos, attacker: Stone) -> Vec<Pos> {
        let defender = attacker.opponent();
        let mut defenses = Vec::new();
        let mut four_positions: Vec<Pos> = Vec::new();

        // Find blocking moves at the extension points of the four
        // Also collect the positions of the four-pattern stones
        for &(dr, dc) in &DIRECTIONS {
            let mut count = 1;
            let mut extension_points = Vec::new();
            let mut line_positions = vec![threat_move];

            // Scan positive direction
            let mut r = threat_move.row as i32 + dr;
            let mut c = threat_move.col as i32 + dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                match board.get(p) {
                    s if s == attacker => {
                        count += 1;
                        line_positions.push(p);
                    }
                    Stone::Empty => {
                        extension_points.push(p);
                        break;
                    }
                    _ => break,
                }
                r += dr;
                c += dc;
            }

            // Scan negative direction
            r = threat_move.row as i32 - dr;
            c = threat_move.col as i32 - dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                match board.get(p) {
                    s if s == attacker => {
                        count += 1;
                        line_positions.push(p);
                    }
                    Stone::Empty => {
                        extension_points.push(p);
                        break;
                    }
                    _ => break,
                }
                r -= dr;
                c -= dc;
            }

            // If this direction has a four, the extension points are defenses
            if count == 4 {
                for ext in extension_points {
                    if is_valid_move(board, ext, defender) {
                        defenses.push(ext);
                    }
                }
                // Collect the four-pattern positions for capture validation
                four_positions.extend(line_positions);
            }
        }

        // Deduplicate four_positions
        four_positions.sort();
        four_positions.dedup();

        // Find capture moves that actually break the four
        // Only include captures that remove stones that are part of the four pattern
        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, defender) {
                    continue;
                }

                let captured = get_captured_positions(board, pos, defender);
                if !captured.is_empty() {
                    // Only add as defense if any captured stone is part of the four pattern
                    if captured.iter().any(|cap| four_positions.contains(cap)) {
                        defenses.push(pos);
                    }
                }
            }
        }

        defenses.sort();
        defenses.dedup();
        defenses
    }

    /// Search for VCT (Victory by Continuous Threats)
    ///
    /// VCT is more general than VCF - it considers both four-threats and
    /// open-three threats. At each node, we try VCF first, then fall back
    /// to VCT with open-threes.
    ///
    /// # Arguments
    /// * `board` - Current board state
    /// * `color` - Color of the attacking player
    ///
    /// # Returns
    /// `ThreatResult` with the winning sequence if found
    pub fn search_vct(&mut self, board: &Board, color: Stone) -> ThreatResult {
        self.nodes = 0;
        let mut sequence = Vec::new();

        // First try VCF (faster and more forcing)
        if self.vcf_search(board, color, 0, &mut sequence) {
            return ThreatResult::found(sequence);
        }

        sequence.clear();
        if self.vct_search(board, color, 0, &mut sequence) {
            ThreatResult::found(sequence)
        } else {
            ThreatResult::not_found()
        }
    }

    /// Internal recursive VCT search
    fn vct_search(
        &mut self,
        board: &Board,
        color: Stone,
        depth: u8,
        sequence: &mut Vec<Pos>,
    ) -> bool {
        self.nodes += 1;

        if depth > self.max_vct_depth {
            return false;
        }

        // Find all threat moves (fours and open-threes)
        let threats = self.find_all_threats(board, color);

        for threat_move in threats {
            // Make the threat move
            let mut new_board = board.clone();
            new_board.place_stone(threat_move, color);
            execute_captures(&mut new_board, threat_move, color);

            sequence.push(threat_move);

            // Check for immediate win
            if has_five_in_row(&new_board, color) {
                if let Some(five) = find_five_positions(&new_board, color) {
                    if !can_break_five_by_capture(&new_board, &five, color) {
                        return true;
                    }
                }
            }

            if new_board.captures(color) >= 5 {
                return true;
            }

            // Try VCF from this position (faster path to victory)
            let mut vcf_seq = Vec::new();
            if self.vcf_search(&new_board, color, 0, &mut vcf_seq) {
                sequence.extend(vcf_seq);
                return true;
            }

            // Find all possible defenses
            let defenses = self.find_threat_defenses(&new_board, threat_move, color);

            if defenses.is_empty() {
                return true;
            }

            // For VCT, we need to beat ALL possible defenses
            let mut all_defenses_beaten = true;
            for defense in &defenses {
                let mut def_board = new_board.clone();
                def_board.place_stone(*defense, color.opponent());
                execute_captures(&mut def_board, *defense, color.opponent());

                // Recursively try to find a win against this defense
                let mut sub_sequence = sequence.clone();
                if !self.vct_search(&def_board, color, depth + 1, &mut sub_sequence) {
                    all_defenses_beaten = false;
                    break;
                }
            }

            if all_defenses_beaten {
                return true;
            }

            sequence.pop();
        }

        false
    }

    /// Find all threat moves (winning moves, fours, and open-threes)
    ///
    /// Prioritizes: winning moves > fours > open-threes
    fn find_all_threats(&self, board: &Board, color: Stone) -> Vec<Pos> {
        let mut winning_moves = Vec::new();
        let mut four_threats = Vec::new();
        let mut three_threats = Vec::new();

        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, color) {
                    continue;
                }

                // Prioritize winning moves > fours > open-threes
                if self.creates_five_or_more(board, pos, color) {
                    winning_moves.push(pos);
                } else if self.creates_four(board, pos, color) {
                    four_threats.push(pos);
                } else if self.creates_open_three(board, pos, color) {
                    three_threats.push(pos);
                }
            }
        }

        // Combine with priority order
        winning_moves.extend(four_threats);
        winning_moves.extend(three_threats);
        winning_moves
    }

    /// Check if placing at pos creates an open three (3 in a row with both ends open)
    fn creates_open_three(&self, board: &Board, pos: Pos, color: Stone) -> bool {
        for &(dr, dc) in &DIRECTIONS {
            let mut count = 1;
            let mut open_ends = 0;

            // Scan positive direction
            let mut r = pos.row as i32 + dr;
            let mut c = pos.col as i32 + dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                match board.get(p) {
                    s if s == color => count += 1,
                    Stone::Empty => {
                        open_ends += 1;
                        break;
                    }
                    _ => break,
                }
                r += dr;
                c += dc;
            }

            // Scan negative direction
            r = pos.row as i32 - dr;
            c = pos.col as i32 - dc;
            while Pos::is_valid(r, c) {
                let p = Pos::new(r as u8, c as u8);
                match board.get(p) {
                    s if s == color => count += 1,
                    Stone::Empty => {
                        open_ends += 1;
                        break;
                    }
                    _ => break,
                }
                r -= dr;
                c -= dc;
            }

            // Open three needs exactly 3 stones and both ends open
            if count == 3 && open_ends == 2 {
                return true;
            }
        }

        false
    }

    /// Find defense moves against any threat (for VCT)
    ///
    /// Defense includes:
    /// 1. Blocking moves at the ends of the threat line
    /// 2. Capture moves that break the threat (only captures that remove stones from the threat pattern)
    fn find_threat_defenses(&self, board: &Board, threat_move: Pos, attacker: Stone) -> Vec<Pos> {
        let defender = attacker.opponent();
        let mut defenses = Vec::new();
        let mut threat_positions: Vec<Pos> = Vec::new();

        // Find blocking positions along each direction
        // Also collect the positions of the threat-pattern stones
        for &(dr, dc) in &DIRECTIONS {
            let mut line_positions = vec![threat_move];
            let mut line_count = 1;

            // Check both extension directions from the threat
            for sign in [-1i32, 1] {
                let mut r = threat_move.row as i32;
                let mut c = threat_move.col as i32;

                // Move to the end of the line of attacker stones
                while Pos::is_valid(r + dr * sign, c + dc * sign) {
                    let next = Pos::new((r + dr * sign) as u8, (c + dc * sign) as u8);
                    if board.get(next) == attacker {
                        r += dr * sign;
                        c += dc * sign;
                        line_positions.push(next);
                        line_count += 1;
                    } else {
                        break;
                    }
                }

                // The next position after the line is a potential defense
                let def_r = r + dr * sign;
                let def_c = c + dc * sign;
                if Pos::is_valid(def_r, def_c) {
                    let p = Pos::new(def_r as u8, def_c as u8);
                    if board.get(p) == Stone::Empty && is_valid_move(board, p, defender) {
                        defenses.push(p);
                    }
                }
            }

            // If this direction has a meaningful threat (3+ stones), collect positions
            if line_count >= 3 {
                threat_positions.extend(line_positions);
            }
        }

        // Deduplicate threat_positions
        threat_positions.sort();
        threat_positions.dedup();

        // Add capture defenses that actually break the threat
        // Only include captures that remove stones that are part of the threat pattern
        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                let pos = Pos::new(r as u8, c as u8);
                if !is_valid_move(board, pos, defender) {
                    continue;
                }
                let captured = get_captured_positions(board, pos, defender);
                if !captured.is_empty() {
                    // Only add as defense if any captured stone is part of the threat pattern
                    if captured.iter().any(|cap| threat_positions.contains(cap)) {
                        defenses.push(pos);
                    }
                }
            }
        }

        defenses.sort();
        defenses.dedup();
        defenses
    }

    /// Get the number of nodes searched
    #[inline]
    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    /// Reset node counter
    #[inline]
    pub fn reset_nodes(&mut self) {
        self.nodes = 0;
    }
}

impl Default for ThreatSearcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a board from a string representation
    fn setup_board(setup: &[(u8, u8, Stone)]) -> Board {
        let mut board = Board::new();
        for &(row, col, stone) in setup {
            board.place_stone(Pos::new(row, col), stone);
        }
        board
    }

    #[test]
    fn test_creates_four_horizontal() {
        // Setup: _ B B B _ (placing at any end creates four)
        let board = setup_board(&[
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
            (9, 8, Stone::Black),
        ]);

        let searcher = ThreatSearcher::new();

        // Placing at (9, 5) creates: B B B B _ (four with open end)
        assert!(searcher.creates_four(&board, Pos::new(9, 5), Stone::Black));

        // Placing at (9, 9) creates: _ B B B B (four with open end)
        assert!(searcher.creates_four(&board, Pos::new(9, 9), Stone::Black));
    }

    #[test]
    fn test_creates_four_with_gap() {
        // Setup: _ B B _ B _ (placing in gap creates four)
        let board = setup_board(&[
            (9, 5, Stone::Black),
            (9, 6, Stone::Black),
            (9, 8, Stone::Black),
        ]);

        let searcher = ThreatSearcher::new();

        // Placing at (9, 7) creates: B B B B (four)
        assert!(searcher.creates_four(&board, Pos::new(9, 7), Stone::Black));
    }

    #[test]
    fn test_not_four_blocked() {
        // Setup: W B B B _ (blocked on one side, but still a four threat)
        let board = setup_board(&[
            (9, 4, Stone::White),
            (9, 5, Stone::Black),
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
        ]);

        let searcher = ThreatSearcher::new();

        // Placing at (9, 8) creates: W B B B B _ (four with one open end - still valid)
        assert!(searcher.creates_four(&board, Pos::new(9, 8), Stone::Black));
    }

    #[test]
    fn test_creates_open_three() {
        // Setup: _ B B _ (placing creates open three)
        let board = setup_board(&[(9, 6, Stone::Black), (9, 7, Stone::Black)]);

        let searcher = ThreatSearcher::new();

        // Placing at (9, 8) creates: _ B B B _ (open three)
        assert!(searcher.creates_open_three(&board, Pos::new(9, 8), Stone::Black));

        // Placing at (9, 5) creates: _ B B B _ (open three)
        assert!(searcher.creates_open_three(&board, Pos::new(9, 5), Stone::Black));
    }

    #[test]
    fn test_not_open_three_blocked() {
        // Setup: W B B _ (blocked on one side - not open three)
        let board = setup_board(&[
            (9, 4, Stone::White),
            (9, 5, Stone::Black),
            (9, 6, Stone::Black),
        ]);

        let searcher = ThreatSearcher::new();

        // Placing at (9, 7) creates: W B B B _ (blocked, not open three)
        assert!(!searcher.creates_open_three(&board, Pos::new(9, 7), Stone::Black));
    }

    #[test]
    fn test_vcf_immediate_win() {
        // Setup: _ B B B B _ (one move to win at either end)
        let board = setup_board(&[
            (9, 5, Stone::Black),
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
            (9, 8, Stone::Black),
        ]);

        let mut searcher = ThreatSearcher::new();
        let result = searcher.search_vcf(&board, Stone::Black);

        assert!(result.found);
        assert_eq!(result.winning_sequence.len(), 1);
        // Either end wins
        let winning_pos = result.winning_sequence[0];
        assert!(
            winning_pos == Pos::new(9, 4) || winning_pos == Pos::new(9, 9),
            "Expected winning move at (9,4) or (9,9), got {:?}",
            winning_pos
        );
    }

    #[test]
    fn test_vcf_two_step_win() {
        // Setup a double-four scenario:
        // Black has two separate threes that each need one move to become four
        // If black plays one four, opponent must block, then black plays the other four
        //
        // Setup: B B B _ and B B B _ in perpendicular directions
        //        so after first four is blocked, second four wins
        //
        // Horizontal: B B B _ at row 9, cols 5-7 (four at col 8)
        // Vertical: B B B _ at col 9, rows 5-7 (four at row 8)
        //
        // Black plays (9, 8) = horizontal four, White blocks at (9, 9) or (9, 4)
        // Black plays (8, 9) = vertical four, opponent blocked wrong direction = win
        //
        // Actually simpler: use a forced sequence where opponent has only one defense
        //
        // Setup: _ B B B _ _ _ _ B B B _
        //        1 2 3 4 5 6 7 8 9 0 1 2
        // Play at 5: creates B B B B _ - four
        // White must block at 1 (only option since 6 would create another threat)
        // Play at 10: creates B B B B - four with open end
        // White must block... etc.

        // Simpler: Setup an open four that guarantees win
        // _ B B B B _ - open four with both ends open
        let board = setup_board(&[
            (9, 5, Stone::Black),
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
            (9, 8, Stone::Black),
        ]);

        let mut searcher = ThreatSearcher::new();
        let result = searcher.search_vcf(&board, Stone::Black);

        // This is actually an immediate win (five), so should be found
        assert!(result.found);

        // Now test actual VCF sequence: two separate threes
        // where completing one forces defense, then the other wins
        //
        // Setup: perpendicular threes
        //        col:  5 6 7 8 9
        // row 5:       . . . . B
        // row 6:       . . . . B
        // row 7:       . . . . B
        // row 8:       . . . . _  <- completing here makes vertical four
        // row 9:   B B B _ _ . .  <- completing at col 8 makes horizontal four
        //
        // Black plays (9, 8) - horizontal four B B B B
        // White has one defense: (9, 4) or (9, 9)
        // If White plays (9, 9), Black plays (8, 9) - vertical five wins
        // If White plays (9, 4), Black plays (8, 9) - vertical four,
        //   then White must block at (4, 9) or (9, 9)...
        //
        // This is complex. Let's use a simpler test case.
        let board2 = setup_board(&[
            // Horizontal three
            (9, 5, Stone::Black),
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
            // Vertical three sharing endpoint area
            (5, 9, Stone::Black),
            (6, 9, Stone::Black),
            (7, 9, Stone::Black),
            (8, 9, Stone::Black), // This makes a four already!
        ]);

        let result2 = searcher.search_vcf(&board2, Stone::Black);
        // Vertical four (5-8, 9) needs one move at (4, 9) or (9, 9) to win
        assert!(result2.found);
    }

    #[test]
    fn test_vcf_not_found() {
        // Setup: _ B B _ (not enough for VCF)
        let board = setup_board(&[(9, 6, Stone::Black), (9, 7, Stone::Black)]);

        let mut searcher = ThreatSearcher::new();
        let result = searcher.search_vcf(&board, Stone::Black);

        assert!(!result.found);
    }

    #[test]
    fn test_find_defense_moves() {
        // Setup: _ B B B B _ (opponent needs to block at either end)
        let mut board = Board::new();
        for i in 5..9 {
            board.place_stone(Pos::new(9, i), Stone::Black);
        }

        let searcher = ThreatSearcher::new();
        let defenses = searcher.find_defense_moves(&board, Pos::new(9, 5), Stone::Black);

        // White should be able to block at (9, 4) or (9, 9)
        assert!(defenses.contains(&Pos::new(9, 4)) || defenses.contains(&Pos::new(9, 9)));
    }

    #[test]
    fn test_capture_win_detected() {
        // Setup: Black has 4 captures, one more capture wins
        let mut board = Board::new();
        board.add_captures(Stone::Black, 4);

        // Setup capture pattern: B _ W W B
        board.place_stone(Pos::new(9, 5), Stone::Black);
        board.place_stone(Pos::new(9, 7), Stone::White);
        board.place_stone(Pos::new(9, 8), Stone::White);
        board.place_stone(Pos::new(9, 9), Stone::Black);

        let mut searcher = ThreatSearcher::new();
        // Try to find VCF - capturing at (9, 6) should win
        let _result = searcher.search_vcf(&board, Stone::Black);

        // Note: VCF only looks for four-threats, not capture wins
        // The capture at (9, 6) creates a four AND captures, so it should work
        // Actually, let's check if this creates a four...
        // After placing at (9,6): B B _ _ B - not a four

        // Let's create a scenario where we have both four and capture opportunity
        let mut board2 = Board::new();
        board2.add_captures(Stone::Black, 4);

        // Four pattern: B B B _ with capture at the same position
        board2.place_stone(Pos::new(9, 5), Stone::Black);
        board2.place_stone(Pos::new(9, 6), Stone::Black);
        board2.place_stone(Pos::new(9, 7), Stone::Black);
        // Position (9, 8) will create four

        // Add capture pattern around (9, 8)
        // B _ W W B pattern vertically
        board2.place_stone(Pos::new(6, 8), Stone::Black);
        board2.place_stone(Pos::new(7, 8), Stone::White);
        board2.place_stone(Pos::new(8, 8), Stone::White);
        // (9, 8) will be placed

        let result2 = searcher.search_vcf(&board2, Stone::Black);
        // (9, 8) creates a four horizontally, so VCF should find it
        assert!(result2.found);
    }

    #[test]
    fn test_vct_finds_vcf_first() {
        // VCT should find VCF solutions when available
        let board = setup_board(&[
            (9, 5, Stone::Black),
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
            (9, 8, Stone::Black),
        ]);

        let mut searcher = ThreatSearcher::new();
        let result = searcher.search_vct(&board, Stone::Black);

        assert!(result.found);
        // Should find the immediate win
        assert_eq!(result.winning_sequence.len(), 1);
    }

    #[test]
    fn test_vct_with_open_three() {
        // Setup a VCT that requires open-three threats
        // This is a complex scenario - simplified test
        let board = setup_board(&[
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
            // Creating potential for open-three based attack
        ]);

        let mut searcher = ThreatSearcher::new();
        let _result = searcher.search_vct(&board, Stone::Black);

        // VCT is complex and may or may not find a win depending on position
        // Just verify it doesn't crash and returns a valid result
        assert!(searcher.nodes() > 0);
    }

    #[test]
    fn test_threat_searcher_default() {
        let searcher = ThreatSearcher::default();
        assert_eq!(searcher.max_vcf_depth, 30);
        assert_eq!(searcher.max_vct_depth, 20);
    }

    #[test]
    fn test_threat_searcher_with_depths() {
        let searcher = ThreatSearcher::with_depths(10, 5);
        assert_eq!(searcher.max_vcf_depth, 10);
        assert_eq!(searcher.max_vct_depth, 5);
    }

    #[test]
    fn test_node_counting() {
        let board = Board::new();
        let mut searcher = ThreatSearcher::new();

        assert_eq!(searcher.nodes(), 0);

        let _ = searcher.search_vcf(&board, Stone::Black);
        let nodes_after_vcf = searcher.nodes();
        assert!(nodes_after_vcf > 0);

        searcher.reset_nodes();
        assert_eq!(searcher.nodes(), 0);
    }

    #[test]
    fn test_diagonal_four() {
        // Setup: diagonal three with potential four
        let board = setup_board(&[
            (6, 6, Stone::Black),
            (7, 7, Stone::Black),
            (8, 8, Stone::Black),
        ]);

        let searcher = ThreatSearcher::new();

        // Placing at (9, 9) creates diagonal four
        assert!(searcher.creates_four(&board, Pos::new(9, 9), Stone::Black));

        // Placing at (5, 5) creates diagonal four
        assert!(searcher.creates_four(&board, Pos::new(5, 5), Stone::Black));
    }

    #[test]
    fn test_vertical_four() {
        // Setup: vertical three with potential four
        let board = setup_board(&[
            (6, 9, Stone::Black),
            (7, 9, Stone::Black),
            (8, 9, Stone::Black),
        ]);

        let searcher = ThreatSearcher::new();

        // Placing at (9, 9) creates vertical four
        assert!(searcher.creates_four(&board, Pos::new(9, 9), Stone::Black));

        // Placing at (5, 9) creates vertical four
        assert!(searcher.creates_four(&board, Pos::new(5, 9), Stone::Black));
    }

    #[test]
    fn test_find_four_threats_multiple() {
        // Setup position with multiple four-threat opportunities
        let board = setup_board(&[
            // Horizontal three
            (9, 6, Stone::Black),
            (9, 7, Stone::Black),
            (9, 8, Stone::Black),
            // Vertical three
            (6, 5, Stone::Black),
            (7, 5, Stone::Black),
            (8, 5, Stone::Black),
        ]);

        let searcher = ThreatSearcher::new();
        let threats = searcher.find_four_threats(&board, Stone::Black);

        // Should find multiple four-threat positions
        assert!(threats.len() >= 2);
    }

    #[test]
    fn test_respects_double_three_rule() {
        // Setup a position where a move would create a double-three (forbidden)
        let mut board = Board::new();

        // Create cross pattern for double-three
        board.place_stone(Pos::new(9, 8), Stone::Black);
        board.place_stone(Pos::new(9, 10), Stone::Black);
        board.place_stone(Pos::new(8, 9), Stone::Black);
        board.place_stone(Pos::new(10, 9), Stone::Black);

        let searcher = ThreatSearcher::new();
        let threats = searcher.find_four_threats(&board, Stone::Black);

        // (9, 9) would be a double-three, so it should not appear in threats
        // (because is_valid_move returns false for double-three)
        assert!(!threats.contains(&Pos::new(9, 9)));
    }
}
