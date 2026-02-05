"""
Heuristic evaluation function for Gomoku.
Evaluates board positions using pattern-based scoring.
"""

from ..game.board import Board, BLACK, WHITE, EMPTY, BOARD_SIZE
from ..game.rules import Rules
from .patterns import PatternScore


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
    # AI must play to WIN - but DEFENSE FIRST to not lose!
    # "You can't win if you lose first"
    ATTACK_WEIGHT = 1.3   # Still aggressive
    DEFENSE_WEIGHT = 1.6  # Defense priority - must not lose
    CAPTURE_WEIGHT = 1.3  # Captures are powerful
    CENTER_WEIGHT = 0.1

    # Winning/losing scores
    WIN_SCORE = 1_000_000
    LOSE_SCORE = -1_000_000

    # Pattern scores for fast evaluation (avoid hardcoding in methods)
    SCORE_FIVE = 500_000
    SCORE_OPEN_FOUR = 100_000
    SCORE_CLOSED_FOUR = 50_000
    SCORE_OPEN_THREE = 10_000
    SCORE_CLOSED_THREE = 1_000
    SCORE_OPEN_TWO = 500
    SCORE_CLOSED_TWO = 50

    # Capture proximity bonus
    CAPTURE_NEAR_WIN_BONUS = 2000
    CAPTURE_NEAR_WIN_DANGER = 2500

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
        Optimized version - scans only stones, not entire board.

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

        # Check for unstoppable threats (open-four)
        # Open-four means opponent wins next turn unless we have 5 or capture
        if self._has_open_four(board, opp_color):
            return self.LOSE_SCORE // 2  # Very bad, almost losing
        if self._has_open_four(board, color):
            return self.WIN_SCORE // 2  # Very good, almost winning

        # Check for threatening patterns (open-three and closed-four)
        # These patterns can lead to unstoppable wins

        # CRITICAL: Check for closed-four (4 in a row with at least one end open)
        # This is an immediate threat that must be blocked!
        if self._has_closed_four(board, opp_color):
            return self.LOSE_SCORE // 3  # = -333,333 (very dangerous)
        if self._has_closed_four(board, color):
            return self.WIN_SCORE // 3  # = +333,333

        # Check for open-three (3 in a row with both ends open)
        # CRITICAL: Check opponent's threat FIRST (defense priority)
        opp_open_threes = self._count_open_threes(board, opp_color)
        if opp_open_threes >= 1:
            return self.LOSE_SCORE // 4  # = -250,000

        my_open_threes = self._count_open_threes(board, color)
        if my_open_threes >= 1:
            return self.WIN_SCORE // 4  # = +250,000

        # Check for closed-three (3 in a row with one end open)
        # Less dangerous but still needs attention
        opp_closed_threes = self._count_closed_threes(board, opp_color)
        if opp_closed_threes >= 2:  # Multiple closed-threes are dangerous
            return self.LOSE_SCORE // 6  # = -166,666

        # Fast evaluation: only scan existing stones
        my_score = self._fast_evaluate(board, color, opp_color)
        opp_score = self._fast_evaluate(board, opp_color, color)

        # Capture bonus
        my_captures = captures.get(color, 0)
        opp_captures = captures.get(opp_color, 0)
        capture_diff = (my_captures - opp_captures) * 500

        # Extra bonus near capture win
        if my_captures >= 8:
            capture_diff += self.CAPTURE_NEAR_WIN_BONUS
        if opp_captures >= 8:
            capture_diff -= self.CAPTURE_NEAR_WIN_DANGER

        return int(my_score * self.ATTACK_WEIGHT -
                   opp_score * self.DEFENSE_WEIGHT + capture_diff)

    def _fast_evaluate(self, board: Board, color: int, opp_color: int) -> int:
        """
        Fast evaluation by iterating only over stones of the given color.
        Uses bit manipulation to find set bits quickly.
        """
        score = 0
        stones = board.black if color == BLACK else board.white
        center = BOARD_SIZE // 2

        # Iterate only over stones (not entire board)
        temp_stones = stones
        while temp_stones:
            # Get lowest set bit position
            bit = (temp_stones & -temp_stones).bit_length() - 1
            row, col = bit // BOARD_SIZE, bit % BOARD_SIZE

            # Center bonus
            dist = abs(row - center) + abs(col - center)
            score += (18 - dist) * 2

            # Check lines through this stone
            for dr, dc in DIRECTIONS:
                line_score = self._evaluate_line_fast(board, row, col, dr, dc,
                                                       color, opp_color)
                score += line_score

            temp_stones &= temp_stones - 1  # Clear lowest set bit

        return score

    def _evaluate_line_fast(self, board: Board, row: int, col: int,
                            dr: int, dc: int, color: int, opp_color: int) -> int:
        """
        Fast line evaluation - count consecutive and potential.
        Only looks at 4 positions in each direction (enough for patterns).
        """
        consecutive = 1
        space_after = 0
        potential = 0

        # Positive direction
        for i in range(1, 5):
            r, c = row + i * dr, col + i * dc
            if not (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE):
                break
            stone = board.get(r, c)
            if stone == color:
                consecutive += 1
            elif stone == EMPTY:
                space_after += 1
                potential = consecutive
                break
            else:  # Opponent
                break

        # Negative direction
        for i in range(1, 5):
            r, c = row - i * dr, col - i * dc
            if not (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE):
                break
            stone = board.get(r, c)
            if stone == color:
                consecutive += 1
            elif stone == EMPTY:
                space_after += 1
                break
            else:  # Opponent
                break

        # Score based on pattern length (high values to prioritize tactical play)
        if consecutive >= 5:
            return self.SCORE_FIVE
        elif consecutive == 4:
            return self.SCORE_OPEN_FOUR if space_after >= 1 else self.SCORE_CLOSED_FOUR
        elif consecutive == 3:
            return self.SCORE_OPEN_THREE if space_after >= 2 else self.SCORE_CLOSED_THREE
        elif consecutive == 2:
            return self.SCORE_OPEN_TWO if space_after >= 2 else self.SCORE_CLOSED_TWO

        return 0

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

    def _count_open_threes(self, board: Board, color: int) -> int:
        """
        Count open-three patterns (3 in a row with both ends open).
        Open-three can become open-four with one move, which is unstoppable.
        """
        count = 0
        stones = board.black if color == BLACK else board.white
        if not stones:
            return 0

        checked = set()  # Avoid counting same pattern multiple times

        temp_stones = stones
        while temp_stones:
            bit = (temp_stones & -temp_stones).bit_length() - 1
            row, col = bit // BOARD_SIZE, bit % BOARD_SIZE

            for dr, dc in DIRECTIONS:
                pattern_key = self._get_line_key(board, row, col, dr, dc, color)
                if pattern_key and pattern_key not in checked:
                    if self._is_open_three_at(board, row, col, dr, dc, color):
                        count += 1
                        checked.add(pattern_key)

            temp_stones &= temp_stones - 1

        return count

    def _get_line_key(self, board: Board, row: int, col: int,
                      dr: int, dc: int, color: int) -> tuple:
        """Get a unique key for a line of stones (for deduplication)."""
        positions = [(row, col)]
        # Extend in positive direction
        r, c = row + dr, col + dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            positions.append((r, c))
            r, c = r + dr, c + dc
        # Extend in negative direction
        r, c = row - dr, col - dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            positions.append((r, c))
            r, c = r - dr, c - dc

        if len(positions) < 3:
            return None
        positions.sort()
        return tuple(positions)

    def _is_open_three_at(self, board: Board, row: int, col: int,
                          dr: int, dc: int, color: int) -> bool:
        """Check if there's an open-three through (row, col) in direction (dr, dc)."""
        # Count consecutive stones including this one
        count = 1

        # Positive direction
        r, c = row + dr, col + dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r + dr, c + dc
        pos_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)
        # Check if there's space for extension (not just one empty)
        pos_space = pos_end_open
        if pos_end_open:
            nr, nc = r + dr, c + dc
            pos_space = not (0 <= nr < BOARD_SIZE and 0 <= nc < BOARD_SIZE and
                            board.get(nr, nc) != EMPTY and board.get(nr, nc) != color)

        # Negative direction
        r, c = row - dr, col - dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r - dr, c - dc
        neg_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)
        neg_space = neg_end_open
        if neg_end_open:
            nr, nc = r - dr, c - dc
            neg_space = not (0 <= nr < BOARD_SIZE and 0 <= nc < BOARD_SIZE and
                            board.get(nr, nc) != EMPTY and board.get(nr, nc) != color)

        # Open-three: exactly 3 consecutive with both ends open and room to grow
        return count == 3 and pos_end_open and neg_end_open and (pos_space or neg_space)

    def _has_open_four(self, board: Board, color: int) -> bool:
        """
        Check if color has an open-four (4 in a row with both ends open).
        Open-four is unstoppable - guaranteed win next move.
        """
        stones = board.black if color == BLACK else board.white
        if not stones:
            return False

        # Check each stone for open-four pattern
        temp_stones = stones
        while temp_stones:
            bit = (temp_stones & -temp_stones).bit_length() - 1
            row, col = bit // BOARD_SIZE, bit % BOARD_SIZE

            for dr, dc in DIRECTIONS:
                if self._is_open_four_at(board, row, col, dr, dc, color):
                    return True

            temp_stones &= temp_stones - 1

        return False

    def _is_open_four_at(self, board: Board, row: int, col: int,
                         dr: int, dc: int, color: int) -> bool:
        """Check if there's an open-four through (row, col) in direction (dr, dc)."""
        # Count consecutive stones including this one
        count = 1

        # Positive direction
        r, c = row + dr, col + dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r + dr, c + dc
        pos_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)

        # Negative direction
        r, c = row - dr, col - dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r - dr, c - dc
        neg_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)

        # Open-four: exactly 4 consecutive with both ends open
        return count == 4 and pos_end_open and neg_end_open

    def _has_closed_four(self, board: Board, color: int) -> bool:
        """
        Check if color has a closed-four (4 in a row with at least one end open).
        Closed-four requires immediate blocking!
        """
        stones = board.black if color == BLACK else board.white
        if not stones:
            return False

        temp_stones = stones
        while temp_stones:
            bit = (temp_stones & -temp_stones).bit_length() - 1
            row, col = bit // BOARD_SIZE, bit % BOARD_SIZE

            for dr, dc in DIRECTIONS:
                if self._is_closed_four_at(board, row, col, dr, dc, color):
                    return True

            temp_stones &= temp_stones - 1

        return False

    def _is_closed_four_at(self, board: Board, row: int, col: int,
                           dr: int, dc: int, color: int) -> bool:
        """Check if there's a closed-four (4 consecutive with at least one end open)."""
        count = 1

        # Positive direction
        r, c = row + dr, col + dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r + dr, c + dc
        pos_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)

        # Negative direction
        r, c = row - dr, col - dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r - dr, c - dc
        neg_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)

        # Closed-four: 4 consecutive with at least one end open (but not both - that's open-four)
        return count == 4 and (pos_end_open or neg_end_open) and not (pos_end_open and neg_end_open)

    def _count_closed_threes(self, board: Board, color: int) -> int:
        """
        Count closed-three patterns (3 in a row with exactly one end open).
        Less dangerous than open-three but still needs attention.
        """
        count = 0
        stones = board.black if color == BLACK else board.white
        if not stones:
            return 0

        checked = set()

        temp_stones = stones
        while temp_stones:
            bit = (temp_stones & -temp_stones).bit_length() - 1
            row, col = bit // BOARD_SIZE, bit % BOARD_SIZE

            for dr, dc in DIRECTIONS:
                pattern_key = self._get_line_key(board, row, col, dr, dc, color)
                if pattern_key and pattern_key not in checked:
                    if self._is_closed_three_at(board, row, col, dr, dc, color):
                        count += 1
                        checked.add(pattern_key)

            temp_stones &= temp_stones - 1

        return count

    def _is_closed_three_at(self, board: Board, row: int, col: int,
                            dr: int, dc: int, color: int) -> bool:
        """Check if there's a closed-three (3 consecutive with exactly one end open)."""
        count = 1

        # Positive direction
        r, c = row + dr, col + dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r + dr, c + dc
        pos_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)

        # Negative direction
        r, c = row - dr, col - dc
        while 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and board.get(r, c) == color:
            count += 1
            r, c = r - dr, c - dc
        neg_end_open = (0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE and
                        board.get(r, c) == EMPTY)

        # Closed-three: exactly 3 consecutive with exactly one end open
        # (XOR: one open but not both)
        return count == 3 and (pos_end_open != neg_end_open)

    def clear_cache(self):
        """Clear the evaluation cache."""
        self._cache.clear()
