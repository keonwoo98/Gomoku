"""
Heuristic evaluation function for Gomoku.
Evaluates board positions using pattern-based scoring.
"""

from ..game.board import Board, BLACK, WHITE, EMPTY, BOARD_SIZE
from ..game.rules import Rules
from .patterns import (
    PatternScore, ATTACK_PATTERNS, get_pattern_score,
    line_to_string, count_pattern
)


# Direction vectors for line extraction
DIRECTIONS = [
    (0, 1),   # Horizontal →
    (1, 0),   # Vertical ↓
    (1, 1),   # Diagonal ↘
    (1, -1),  # Diagonal ↗
]


class Heuristic:
    """
    Evaluates board positions for the AI.
    Uses pattern-based scoring with capture consideration.
    """

    # Weights for different factors
    ATTACK_WEIGHT = 1.0
    DEFENSE_WEIGHT = 1.1  # Slightly prioritize defense
    CAPTURE_WEIGHT = 1.2
    CENTER_WEIGHT = 0.1

    # Winning/losing scores
    WIN_SCORE = 1_000_000
    LOSE_SCORE = -1_000_000

    def __init__(self):
        # Cache for evaluated positions
        self._cache = {}

        # Precompute center distances
        center = BOARD_SIZE // 2
        self._center_bonus = {}
        for r in range(BOARD_SIZE):
            for c in range(BOARD_SIZE):
                dist = abs(r - center) + abs(c - center)
                # Max distance is 18, so bonus ranges from 0 to ~50
                self._center_bonus[(r, c)] = max(0, (18 - dist) * 3)

    def evaluate(self, board: Board, color: int, captures: dict) -> int:
        """
        Evaluate the board position from color's perspective.

        Args:
            board: Current board state
            color: The color to evaluate for (BLACK or WHITE)
            captures: Dictionary of capture counts {BLACK: n, WHITE: m}

        Returns:
            Integer score (positive = good for color, negative = bad)
        """
        opp_color = WHITE if color == BLACK else BLACK

        # Check for immediate wins/losses
        if captures.get(color, 0) >= 10:
            return self.WIN_SCORE
        if captures.get(opp_color, 0) >= 10:
            return self.LOSE_SCORE

        if board.has_five_in_row(color):
            return self.WIN_SCORE
        if board.has_five_in_row(opp_color):
            return self.LOSE_SCORE

        # Pattern-based evaluation
        my_score = self._evaluate_patterns(board, color, opp_color)
        opp_score = self._evaluate_patterns(board, opp_color, color)

        # Capture evaluation
        capture_score = self._evaluate_captures(board, color, opp_color, captures)

        # Position evaluation (center control)
        position_score = self._evaluate_positions(board, color, opp_color)

        # Combine scores
        total = (my_score * self.ATTACK_WEIGHT -
                 opp_score * self.DEFENSE_WEIGHT +
                 capture_score * self.CAPTURE_WEIGHT +
                 position_score * self.CENTER_WEIGHT)

        return int(total)

    def _evaluate_patterns(self, board: Board, color: int, opp_color: int) -> int:
        """Evaluate pattern-based score for a color."""
        total_score = 0

        # Scan all lines on the board
        for row in range(BOARD_SIZE):
            for col in range(BOARD_SIZE):
                if board.get(row, col) != color:
                    continue

                # Check each direction from this stone
                for dr, dc in DIRECTIONS:
                    line = self._extract_line(board, row, col, dr, dc, 9)
                    score = get_pattern_score(line, color, opp_color)
                    total_score += score

        # Divide by 2 because patterns are counted from both ends
        return total_score // 2

    def _extract_line(self, board: Board, row: int, col: int,
                      dr: int, dc: int, length: int) -> list:
        """Extract a line of stones centered at (row, col)."""
        half = length // 2
        line = []

        for i in range(-half, half + 1):
            r, c = row + i * dr, col + i * dc
            if 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE:
                line.append(board.get(r, c))
            else:
                line.append(-1)  # Out of bounds marker

        return line

    def _evaluate_captures(self, board: Board, color: int, opp_color: int,
                          captures: dict) -> int:
        """Evaluate capture-related scoring."""
        score = 0

        my_captures = captures.get(color, 0)
        opp_captures = captures.get(opp_color, 0)

        # Score for captures already made
        score += my_captures * PatternScore.CAPTURE_MADE
        score -= opp_captures * PatternScore.CAPTURE_MADE

        # Bonus for being close to capture win
        if my_captures >= 8:
            score += PatternScore.FOUR_CAPTURES
        if opp_captures >= 8:
            score -= PatternScore.FOUR_CAPTURES

        # Count capture threats
        my_threats = self._count_capture_threats(board, color)
        opp_threats = self._count_capture_threats(board, opp_color)

        score += my_threats * PatternScore.CAPTURE_THREAT
        score -= opp_threats * PatternScore.CAPTURE_DANGER

        return score

    def _count_capture_threats(self, board: Board, color: int) -> int:
        """Count number of positions where color can capture."""
        threats = 0
        candidates = board.get_adjacent_empty(radius=1)

        for row, col in candidates:
            captures = Rules.check_captures(board, row, col, color)
            threats += len(captures)

        return threats

    def _evaluate_positions(self, board: Board, color: int, opp_color: int) -> int:
        """Evaluate positional factors like center control."""
        score = 0

        for row in range(BOARD_SIZE):
            for col in range(BOARD_SIZE):
                stone = board.get(row, col)
                bonus = self._center_bonus[(row, col)]

                if stone == color:
                    score += bonus
                elif stone == opp_color:
                    score -= bonus

        return score

    def evaluate_move(self, board: Board, row: int, col: int, color: int,
                      captures: dict) -> int:
        """
        Quick evaluation of a single move (for move ordering).
        Does not fully evaluate the resulting position.
        """
        score = 0
        opp_color = WHITE if color == BLACK else BLACK

        # Check for winning move
        board.place_stone(row, col, color)
        if board.has_five_in_row(color):
            board.remove_stone(row, col)
            return self.WIN_SCORE

        board.remove_stone(row, col)

        # Check for blocking opponent's win
        board.place_stone(row, col, opp_color)
        if board.has_five_in_row(opp_color):
            score += PatternScore.OPEN_FOUR  # High priority to block
        board.remove_stone(row, col)

        # Capture opportunities
        capture_positions = Rules.get_captured_positions(board, row, col, color)
        score += len(capture_positions) * PatternScore.CAPTURE_THREAT

        # Center bonus
        score += self._center_bonus[(row, col)]

        # Quick pattern check around the move
        for dr, dc in DIRECTIONS:
            # Count our stones in line
            count = 1
            open_ends = 0

            # Positive direction
            r, c = row + dr, col + dc
            while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE:
                if board.get(r, c) == color:
                    count += 1
                elif board.get(r, c) == EMPTY:
                    open_ends += 1
                    break
                else:
                    break
                r, c = r + dr, c + dc

            # Negative direction
            r, c = row - dr, col - dc
            while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE:
                if board.get(r, c) == color:
                    count += 1
                elif board.get(r, c) == EMPTY:
                    open_ends += 1
                    break
                else:
                    break
                r, c = r - dr, c - dc

            # Score based on count and open ends
            if count >= 5:
                score += PatternScore.FIVE
            elif count == 4:
                score += PatternScore.OPEN_FOUR if open_ends == 2 else PatternScore.FOUR
            elif count == 3:
                score += PatternScore.OPEN_THREE if open_ends == 2 else PatternScore.THREE
            elif count == 2:
                score += PatternScore.OPEN_TWO if open_ends == 2 else PatternScore.TWO

        return score

    def clear_cache(self):
        """Clear the evaluation cache."""
        self._cache.clear()
