"""
Gomoku game rules implementation.
Includes capture, double-three prohibition, and win conditions.
"""

from .board import Board, EMPTY, BLACK, WHITE, BOARD_SIZE

# 8 directions for checking: (dr, dc)
DIRECTIONS_8 = [
    (-1, 0), (1, 0),    # vertical
    (0, -1), (0, 1),    # horizontal
    (-1, -1), (1, 1),   # diagonal \
    (-1, 1), (1, -1),   # diagonal /
]

# 4 directions (one per axis)
DIRECTIONS_4 = [
    (0, 1),    # horizontal →
    (1, 0),    # vertical ↓
    (1, 1),    # diagonal ↘
    (1, -1),   # diagonal ↗
]


class Rules:
    """Game rules for Gomoku with capture and double-three prohibition."""

    WIN_CAPTURES = 10  # Number of captured stones to win

    @staticmethod
    def opposite(color: int) -> int:
        """Get the opposite color."""
        return WHITE if color == BLACK else BLACK

    @staticmethod
    def check_captures(board: Board, row: int, col: int, color: int) -> list:
        """
        Check for captures that would occur if a stone is placed at (row, col).
        Returns list of captured position pairs: [((r1,c1), (r2,c2)), ...]

        Capture pattern: MY - OPP - OPP - MY
        """
        captures = []
        opp_color = Rules.opposite(color)

        for dr, dc in DIRECTIONS_8:
            # Check positions: (row, col) is MY stone being placed
            # Need: MY at (row, col), OPP at +1, OPP at +2, MY at +3
            r1, c1 = row + dr, col + dc          # First opponent
            r2, c2 = row + 2 * dr, col + 2 * dc  # Second opponent
            r3, c3 = row + 3 * dr, col + 3 * dc  # My stone (must exist)

            if not Board.is_valid_pos(r3, c3):
                continue

            if (board.get(r1, c1) == opp_color and
                board.get(r2, c2) == opp_color and
                board.get(r3, c3) == color):
                captures.append(((r1, c1), (r2, c2)))

        return captures

    @staticmethod
    def get_captured_positions(board: Board, row: int, col: int, color: int) -> list:
        """Get flat list of positions that would be captured."""
        captures = Rules.check_captures(board, row, col, color)
        positions = []
        for pair in captures:
            positions.extend(pair)
        return positions

    @staticmethod
    def count_consecutive(board: Board, row: int, col: int, color: int,
                          dr: int, dc: int) -> int:
        """
        Count consecutive stones in one direction from (row, col).
        Does not include the starting position.
        """
        count = 0
        r, c = row + dr, col + dc

        while Board.is_valid_pos(r, c) and board.get(r, c) == color:
            count += 1
            r += dr
            c += dc

        return count

    @staticmethod
    def count_line(board: Board, row: int, col: int, color: int,
                   dr: int, dc: int) -> tuple:
        """
        Count total consecutive stones through (row, col) in a line.
        Returns (total_count, open_ends)
        open_ends: 0, 1, or 2 (number of open ends)
        """
        # Count in positive direction
        pos_count = Rules.count_consecutive(board, row, col, color, dr, dc)

        # Count in negative direction
        neg_count = Rules.count_consecutive(board, row, col, color, -dr, -dc)

        total = pos_count + neg_count + 1  # +1 for the stone at (row, col)

        # Check open ends
        open_ends = 0

        # Positive end
        end_r, end_c = row + (pos_count + 1) * dr, col + (pos_count + 1) * dc
        if Board.is_valid_pos(end_r, end_c) and board.get(end_r, end_c) == EMPTY:
            open_ends += 1

        # Negative end
        end_r, end_c = row - (neg_count + 1) * dr, col - (neg_count + 1) * dc
        if Board.is_valid_pos(end_r, end_c) and board.get(end_r, end_c) == EMPTY:
            open_ends += 1

        return (total, open_ends)

    @staticmethod
    def check_five_at(board: Board, row: int, col: int, color: int) -> bool:
        """Check if there's 5 or more in a row through (row, col)."""
        for dr, dc in DIRECTIONS_4:
            total, _ = Rules.count_line(board, row, col, color, dr, dc)
            if total >= 5:
                return True
        return False

    @staticmethod
    def is_free_three(board: Board, row: int, col: int, color: int,
                      dr: int, dc: int) -> bool:
        """
        Check if placing a stone creates a free-three in the given direction.
        Free-three: 3 stones that can become an unstoppable open-four.

        Patterns that count as free-three:
        - _XXX_ (open three, consecutive)
        - _XX_X_ or _X_XX_ (open three with gap)
        """
        # Temporarily place the stone
        board.place_stone(row, col, color)

        result = Rules._check_free_three_pattern(board, row, col, color, dr, dc)

        # Remove the stone
        board.remove_stone(row, col)

        return result

    @staticmethod
    def _check_free_three_pattern(board: Board, row: int, col: int, color: int,
                                   dr: int, dc: int) -> bool:
        """Check free-three pattern in a direction (stone already placed)."""
        # Extract line centered at position
        line = []
        for i in range(-5, 6):
            r, c = row + i * dr, col + i * dc
            if Board.is_valid_pos(r, c):
                line.append(board.get(r, c))
            else:
                line.append(None)  # Out of bounds (treated as blocked)

        # Find the stone position in line (index 5)
        center = 5

        # Check consecutive pattern: _XXX_
        # Find the extent of consecutive stones including center
        left = center
        while left > 0 and line[left - 1] == color:
            left -= 1
        right = center
        while right < len(line) - 1 and line[right + 1] == color:
            right += 1

        consecutive = right - left + 1

        if consecutive == 3:
            # Check if both ends are open (empty and in bounds)
            left_open = left > 0 and line[left - 1] == EMPTY
            right_open = right < len(line) - 1 and line[right + 1] == EMPTY

            # Additional check: need space beyond the empty to make open-four
            # Pattern: __XXX__ or at least _XXX_ with room to grow
            if left_open and right_open:
                # Check if there's more space for open-four
                far_left = left > 1 and line[left - 2] != Rules.opposite(color)
                far_right = right < len(line) - 2 and line[right + 2] != Rules.opposite(color)
                if far_left or far_right:
                    return True

        # Check gap patterns: _X_XX_ or _XX_X_
        # Look for patterns where adding one stone creates an open four
        for pattern_start in range(max(0, center - 4), min(len(line) - 5, center + 1)):
            segment = line[pattern_start:pattern_start + 6]
            if None in segment:
                continue

            opp = Rules.opposite(color)
            if opp in segment:
                continue

            # Count our stones and empty spaces
            stone_positions = [i for i, s in enumerate(segment) if s == color]
            empty_positions = [i for i, s in enumerate(segment) if s == EMPTY]

            # Check if center-relative position is in this segment
            center_in_segment = center - pattern_start
            if center_in_segment not in stone_positions:
                continue

            # Pattern _X_XX_ or _XX_X_: 3 stones with specific gap
            if len(stone_positions) == 3 and len(empty_positions) == 3:
                # Check valid free-three gap patterns
                if segment[0] == EMPTY and segment[5] == EMPTY:
                    # Possible patterns: _X_XX_, _XX_X_
                    if (stone_positions == [1, 3, 4] or  # _X_XX_
                        stone_positions == [1, 2, 4]):   # _XX_X_
                        return True

        return False

    @staticmethod
    def count_free_threes(board: Board, row: int, col: int, color: int) -> int:
        """Count how many free-threes would be created by placing at (row, col)."""
        count = 0
        for dr, dc in DIRECTIONS_4:
            if Rules.is_free_three(board, row, col, color, dr, dc):
                count += 1
        return count

    @staticmethod
    def is_double_three(board: Board, row: int, col: int, color: int) -> bool:
        """Check if placing a stone creates a double-three (forbidden)."""
        return Rules.count_free_threes(board, row, col, color) >= 2

    @staticmethod
    def can_break_five(board: Board, five_positions: list, color: int) -> bool:
        """
        Check if opponent can break a five-in-row by capture.
        five_positions: list of (row, col) forming the five
        color: the color that made the five

        Optimized: Only check positions adjacent to five stones (max 30 positions)
        instead of all 361 board positions.
        """
        opp_color = Rules.opposite(color)
        five_set = set(five_positions)

        # Only check empty positions near the five-in-row
        candidates = set()
        for row, col in five_positions:
            for dr, dc in DIRECTIONS_8:
                for dist in range(1, 4):  # Check up to 3 positions away
                    nr, nc = row + dr * dist, col + dc * dist
                    if Board.is_valid_pos(nr, nc) and board.is_empty(nr, nc):
                        candidates.add((nr, nc))

        for row, col in candidates:
            captures = Rules.get_captured_positions(board, row, col, opp_color)
            for cap_pos in captures:
                if cap_pos in five_set:
                    return True

        return False

    @staticmethod
    def get_five_positions(board: Board, row: int, col: int, color: int) -> list:
        """Get positions forming a five-in-row through (row, col)."""
        for dr, dc in DIRECTIONS_4:
            positions = [(row, col)]

            # Positive direction
            r, c = row + dr, col + dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                positions.append((r, c))
                r, c = r + dr, c + dc

            # Negative direction
            r, c = row - dr, col - dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                positions.append((r, c))
                r, c = r - dr, c - dc

            if len(positions) >= 5:
                return positions

        return []

    @staticmethod
    def is_valid_move(board: Board, row: int, col: int, color: int) -> bool:
        """
        Check if a move is valid.
        - Must be empty
        - Must not create double-three (unless capturing)
        """
        if not Board.is_valid_pos(row, col):
            return False
        if not board.is_empty(row, col):
            return False

        # Check for captures first
        captures = Rules.check_captures(board, row, col, color)

        # Double-three is allowed if it results in a capture
        if len(captures) == 0:
            if Rules.is_double_three(board, row, col, color):
                return False

        return True

    @staticmethod
    def get_invalid_reason(board: Board, row: int, col: int, color: int) -> str:
        """
        Get the reason why a move is invalid.
        Returns empty string if move is valid.
        """
        if not Board.is_valid_pos(row, col):
            return "Out of bounds"
        if not board.is_empty(row, col):
            return "Position occupied"

        # Check for captures first
        captures = Rules.check_captures(board, row, col, color)

        # Double-three check
        if len(captures) == 0:
            if Rules.is_double_three(board, row, col, color):
                return "Double-three forbidden"

        return ""  # Valid move

    @staticmethod
    def check_winner(board: Board, row: int, col: int, color: int,
                     captures: dict) -> int:
        """
        Check if there's a winner after a move at (row, col).
        captures: {BLACK: count, WHITE: count}
        Returns: BLACK, WHITE, or EMPTY (no winner)

        Checks in order:
        1. Capture wins (either player)
        2. Opponent's EXISTING five-in-row (they won on previous turn)
        3. Current player's NEW five-in-row
        """
        opp_color = Rules.opposite(color)

        # Check capture win for current player
        if captures.get(color, 0) >= Rules.WIN_CAPTURES:
            return color

        # Check capture win for opponent
        if captures.get(opp_color, 0) >= Rules.WIN_CAPTURES:
            return opp_color

        # CRITICAL: Check opponent's EXISTING five-in-row FIRST
        # If opponent already had five before this move, they won earlier
        # (This move shouldn't have been allowed, but check anyway)
        if board.has_five_in_row(opp_color):
            # Verify the current move didn't break it by capture
            # If opponent still has five, opponent wins
            return opp_color

        # Check current player's five-in-row
        # CRITICAL: Check ENTIRE BOARD, not just the last move position!
        # AI might already have 5-in-row but played elsewhere
        if board.has_five_in_row(color):
            # Find the five-in-row positions for endgame capture check
            five_positions = Rules._find_any_five_positions(board, color)

            if five_positions:
                # If opponent can break the five by capture, no win yet
                if Rules.can_break_five(board, five_positions, color):
                    return EMPTY

                # If opponent has 8+ captures and can reach 10, they win
                if captures.get(opp_color, 0) >= 8:
                    # Check if opponent can capture to 10
                    for r in range(BOARD_SIZE):
                        for c in range(BOARD_SIZE):
                            if board.is_empty(r, c):
                                opp_captures = Rules.check_captures(board, r, c, opp_color)
                                if captures.get(opp_color, 0) + len(opp_captures) * 2 >= 10:
                                    return opp_color

            return color

        return EMPTY

    @staticmethod
    def _find_any_five_positions(board: Board, color: int) -> list:
        """Find positions of any five-in-row for the given color."""
        # Scan all positions to find a five-in-row
        for r in range(BOARD_SIZE):
            for c in range(BOARD_SIZE):
                if board.get(r, c) == color:
                    positions = Rules.get_five_positions(board, r, c, color)
                    if positions and len(positions) >= 5:
                        return positions
        return []

    @staticmethod
    def get_valid_moves(board: Board, color: int) -> list:
        """Get all valid moves for a color."""
        moves = []
        candidates = board.get_adjacent_empty(radius=2)

        for row, col in candidates:
            if Rules.is_valid_move(board, row, col, color):
                moves.append((row, col))

        return moves

    @staticmethod
    def is_game_over(board: Board, captures: dict) -> tuple:
        """
        Check if game is over.
        Returns: (is_over, winner) where winner is BLACK, WHITE, or EMPTY (draw)
        """
        # Check capture win
        for color in [BLACK, WHITE]:
            if captures.get(color, 0) >= Rules.WIN_CAPTURES:
                return (True, color)

        # Check five-in-row for both colors
        for color in [BLACK, WHITE]:
            if board.has_five_in_row(color):
                return (True, color)

        # Check for draw (board full - very rare in Gomoku)
        if board.count_stones(BLACK) + board.count_stones(WHITE) >= BOARD_SIZE * BOARD_SIZE:
            return (True, EMPTY)

        return (False, EMPTY)
