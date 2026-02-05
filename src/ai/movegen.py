"""
Move generation and ordering for Gomoku AI.
Implements various heuristics for efficient move ordering.
"""

from typing import Optional
from ..game.board import Board, BLACK, WHITE, EMPTY, BOARD_SIZE
from ..game.rules import Rules
from .heuristic import Heuristic


class MoveGenerator:
    """
    Generates and orders moves for the AI.
    Uses multiple heuristics for optimal Alpha-Beta pruning.
    """

    # Configuration
    MAX_KILLERS = 3  # Killer moves per depth
    HISTORY_MAX = 10000  # Cap to prevent overflow
    HISTORY_DECAY = 2  # Divide by this on aging

    def __init__(self, heuristic: Heuristic):
        self.heuristic = heuristic

        # Killer moves: moves that caused cutoffs at each depth
        self.killer_moves = {}  # depth -> [move1, move2, move3]

        # History heuristic: accumulated scores for moves
        self.history = {}  # move -> score

        # Countermove heuristic: response that worked after opponent's move
        self.countermoves = {}  # opponent_move -> our_best_response

        # Previous best move from iterative deepening
        self.prev_best = None

        # Last opponent move (for countermove heuristic)
        self.last_opponent_move = None

    # Maximum moves to consider at each depth
    MAX_MOVES_ROOT = 30  # More moves at root for accuracy
    MAX_MOVES_DEEP = 15  # Fewer moves at deeper nodes for speed

    def get_moves(self, board: Board, color: int, depth: int,
                  captures: dict, prev_best: Optional[tuple] = None,
                  tt_move: Optional[tuple] = None) -> list:
        """
        Get ordered list of candidate moves.

        Args:
            board: Current board state
            color: Color to generate moves for
            depth: Current search depth
            captures: Current capture counts
            prev_best: Best move from previous iteration (iterative deepening)
            tt_move: Best move from transposition table

        Returns:
            List of (row, col) tuples, ordered by expected quality
        """
        opp_color = WHITE if color == BLACK else BLACK

        # CRITICAL: First check for immediate threats that MUST be addressed
        # This ensures we don't miss winning/blocking moves due to radius limits
        critical_moves = self._find_critical_moves(board, color, opp_color)

        # Get all candidate positions (near existing stones)
        # Use smaller radius at deeper depths for pruning
        radius = 2 if depth >= 3 else 1
        candidates = board.get_adjacent_empty(radius=radius)

        # Merge critical moves into candidates (they might be outside radius)
        candidates = candidates.union(critical_moves)

        # If board is empty, return center
        if not candidates:
            center = BOARD_SIZE // 2
            return [(center, center)]

        # Filter for valid moves only
        valid_moves = [
            (r, c) for r, c in candidates
            if Rules.is_valid_move(board, r, c, color)
        ]

        if not valid_moves:
            return []

        # Score and sort moves
        scored_moves = []
        for move in valid_moves:
            score = self._score_move(board, move, color, depth, captures,
                                     prev_best, tt_move)
            scored_moves.append((score, move))

        # Sort by score (highest first)
        scored_moves.sort(reverse=True, key=lambda x: x[0])

        # Limit number of moves based on depth
        max_moves = self.MAX_MOVES_ROOT if depth >= 3 else self.MAX_MOVES_DEEP
        moves = [move for score, move in scored_moves[:max_moves]]

        return moves

    def _score_move(self, board: Board, move: tuple, color: int,
                    depth: int, captures: dict, prev_best: Optional[tuple],
                    tt_move: Optional[tuple] = None) -> int:
        """
        Score a move for ordering purposes.
        Prioritizes tactical moves (wins, blocks) over positional heuristics.
        """
        row, col = move
        opp_color = WHITE if color == BLACK else BLACK

        # Priority 0: Check for IMMEDIATE WINNING MOVE
        board.place_stone(row, col, color)
        if board.has_five_in_row(color):
            board.remove_stone(row, col)
            return 100_000_000  # Highest priority - we win!
        board.remove_stone(row, col)

        # Priority 0.5: Check for BLOCKING OPPONENT'S WIN
        board.place_stone(row, col, opp_color)
        if board.has_five_in_row(opp_color):
            board.remove_stone(row, col)
            return 90_000_000  # Must block or we lose!
        board.remove_stone(row, col)

        # Priority 1: Check for creating OPEN FOUR (unstoppable threat)
        board.place_stone(row, col, color)
        if self._creates_open_four(board, row, col, color):
            board.remove_stone(row, col)
            return 80_000_000
        board.remove_stone(row, col)

        # Priority 1.5: Block opponent's open four threat
        board.place_stone(row, col, opp_color)
        if self._creates_open_four(board, row, col, opp_color):
            board.remove_stone(row, col)
            return 70_000_000
        board.remove_stone(row, col)

        # Priority 1.6: Block opponent's closed-four threat (4 in a row, at least one end open)
        # This is CRITICAL - higher than our own open-four because opponent moves first!
        board.place_stone(row, col, opp_color)
        if self._creates_closed_four(board, row, col, opp_color):
            board.remove_stone(row, col)
            return 85_000_000  # Higher than our open-four (80M) - defense first!
        board.remove_stone(row, col)

        # Priority 1.7: Block opponent's three-to-four extension
        # If opponent places here, they go from 3 to 4 - very dangerous!
        board.place_stone(row, col, opp_color)
        if self._creates_three_in_row(board, row, col, opp_color):
            board.remove_stone(row, col)
            return 55_000_000  # Block 3->4 extension early!
        board.remove_stone(row, col)

        # Priority 1.8: Block opponent's two-to-three extension (open two becoming open three)
        # Preventive defense - stop threats before they become dangerous!
        board.place_stone(row, col, opp_color)
        if self._creates_open_two_extension(board, row, col, opp_color):
            board.remove_stone(row, col)
            return 30_000_000  # Block 2->3 extension proactively!
        board.remove_stone(row, col)

        # Priority 2: Capture move that wins
        capture_positions = Rules.get_captured_positions(board, row, col, color)
        if capture_positions:
            new_captures = captures.get(color, 0) + len(capture_positions)
            if new_captures >= 10:
                return 95_000_000  # Win by capture

        # Priority levels for move ordering heuristics
        # 3. TT move
        if tt_move and move == tt_move:
            return 20_000_000

        # 4. Previous iteration's best
        if prev_best and move == prev_best:
            return 10_000_000

        # 3. Killer moves
        if depth in self.killer_moves and move in self.killer_moves[depth]:
            return 5_000_000 + self.history.get(move, 0)

        # 4. Countermove
        countermove = self.get_countermove(self.last_opponent_move)
        if countermove and move == countermove:
            return 4_000_000 + self.history.get(move, 0)

        # For other moves, use simple heuristics
        score = 0

        # History heuristic (very important for move ordering)
        score += self.history.get(move, 0) * 100

        # Center distance (center positions are generally better)
        center = BOARD_SIZE // 2
        dist = abs(row - center) + abs(col - center)
        score += (20 - dist) * 50

        # Adjacent stone bonus (prefer moves near existing stones)
        # This is implicitly handled by get_adjacent_empty, but we can weight it
        adjacency_bonus = 0
        for dr in [-1, 0, 1]:
            for dc in [-1, 0, 1]:
                if dr == 0 and dc == 0:
                    continue
                r, c = row + dr, col + dc
                if Board.is_valid_pos(r, c):
                    stone = board.get(r, c)
                    if stone == color:
                        adjacency_bonus += 200  # Adjacent to own stone
                    elif stone != EMPTY:
                        adjacency_bonus += 100  # Adjacent to opponent

        score += adjacency_bonus

        return score

    def _find_critical_moves(self, board: Board, color: int, opp_color: int) -> set:
        """
        Find critical moves that MUST be considered regardless of radius.
        Scans sliding windows of 5 across all lines to find:
        1. Winning moves (complete 5 in a row)
        2. Blocking moves (prevent opponent's 5 in a row)
        3. Strong threats (4 in a row with gaps)
        """
        critical = set()
        directions = [(0, 1), (1, 0), (1, 1), (1, -1)]

        # Scan all possible 5-position windows
        for r in range(BOARD_SIZE):
            for c in range(BOARD_SIZE):
                for dr, dc in directions:
                    # Check if this 5-window fits on board
                    end_r, end_c = r + 4 * dr, c + 4 * dc
                    if not Board.is_valid_pos(end_r, end_c):
                        continue

                    # Analyze the 5-position window
                    window = []
                    positions = []
                    for i in range(5):
                        pr, pc = r + i * dr, c + i * dc
                        window.append(board.get(pr, pc))
                        positions.append((pr, pc))

                    # Check for both colors
                    for check_color in [color, opp_color]:
                        other = opp_color if check_color == color else color
                        color_count = window.count(check_color)
                        empty_count = window.count(EMPTY)
                        other_count = window.count(other)

                        # CRITICAL: 4 stones + 1 empty = winning/blocking move
                        if color_count == 4 and empty_count == 1 and other_count == 0:
                            for i, v in enumerate(window):
                                if v == EMPTY:
                                    critical.add(positions[i])

                        # IMPORTANT: 3 stones + 2 empty = potential open-four
                        if color_count == 3 and empty_count == 2 and other_count == 0:
                            for i, v in enumerate(window):
                                if v == EMPTY:
                                    critical.add(positions[i])

        return critical

    def _creates_open_four(self, board: Board, row: int, col: int, color: int) -> bool:
        """Check if placing at (row, col) creates an open four (4 with both ends open)."""
        for dr, dc in [(0, 1), (1, 0), (1, 1), (1, -1)]:
            count = 1
            open_ends = 0

            # Positive direction
            r, c = row + dr, col + dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r + dr, c + dc
            if Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY:
                open_ends += 1

            # Negative direction
            r, c = row - dr, col - dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r - dr, c - dc
            if Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY:
                open_ends += 1

            if count == 4 and open_ends == 2:
                return True

        return False

    def _creates_closed_four(self, board: Board, row: int, col: int, color: int) -> bool:
        """Check if placing at (row, col) creates a closed four (4 with at least one end open)."""
        for dr, dc in [(0, 1), (1, 0), (1, 1), (1, -1)]:
            count = 1
            open_ends = 0

            # Positive direction
            r, c = row + dr, col + dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r + dr, c + dc
            if Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY:
                open_ends += 1

            # Negative direction
            r, c = row - dr, col - dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r - dr, c - dc
            if Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY:
                open_ends += 1

            # Closed-four: 4 consecutive with at least one end open (includes open-four)
            if count == 4 and open_ends >= 1:
                return True

        return False

    def _creates_three_in_row(self, board: Board, row: int, col: int, color: int) -> bool:
        """Check if placing at (row, col) creates a three with at least one end open.
        This means opponent is building toward a four - need to block early!"""
        for dr, dc in [(0, 1), (1, 0), (1, 1), (1, -1)]:
            count = 1
            open_ends = 0

            # Positive direction
            r, c = row + dr, col + dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r + dr, c + dc
            if Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY:
                open_ends += 1

            # Negative direction
            r, c = row - dr, col - dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r - dr, c - dc
            if Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY:
                open_ends += 1

            # Three with at least one open end - opponent can extend to four
            if count == 3 and open_ends >= 1:
                return True

        return False

    def _creates_open_two_extension(self, board: Board, row: int, col: int, color: int) -> bool:
        """Check if placing at (row, col) extends an open two to an open three.
        This is preventive defense - stop patterns before they become dangerous!"""
        for dr, dc in [(0, 1), (1, 0), (1, 1), (1, -1)]:
            count = 1
            open_ends = 0

            # Positive direction
            r, c = row + dr, col + dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r + dr, c + dc
            pos_open = Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY
            if pos_open:
                open_ends += 1
                # Check for additional space beyond
                nr, nc = r + dr, c + dc
                if Board.is_valid_pos(nr, nc) and board.get(nr, nc) == EMPTY:
                    open_ends += 1  # Extra space = more dangerous

            # Negative direction
            r, c = row - dr, col - dc
            while Board.is_valid_pos(r, c) and board.get(r, c) == color:
                count += 1
                r, c = r - dr, c - dc
            neg_open = Board.is_valid_pos(r, c) and board.get(r, c) == EMPTY
            if neg_open:
                open_ends += 1
                # Check for additional space beyond
                nr, nc = r - dr, c - dc
                if Board.is_valid_pos(nr, nc) and board.get(nr, nc) == EMPTY:
                    open_ends += 1  # Extra space = more dangerous

            # Open three (3 stones with both ends open) - very dangerous pattern
            if count == 3 and open_ends >= 3:  # Both ends open with room to grow
                return True

        return False

    def record_killer(self, move: tuple, depth: int):
        """Record a killer move at a depth."""
        if depth not in self.killer_moves:
            self.killer_moves[depth] = []

        killers = self.killer_moves[depth]
        if move not in killers:
            killers.insert(0, move)
            if len(killers) > self.MAX_KILLERS:
                killers.pop()

    def update_history(self, move: tuple, depth: int):
        """Update history score for a move with depth-squared bonus."""
        if move not in self.history:
            self.history[move] = 0

        # Depth-squared bonus (prefer deeper cutoffs)
        bonus = depth * depth
        self.history[move] += bonus

        # Cap to prevent overflow
        if self.history[move] > self.HISTORY_MAX:
            self.history[move] = self.HISTORY_MAX

    def record_countermove(self, opponent_move: Optional[tuple], our_move: tuple):
        """Record a successful countermove response."""
        if opponent_move is not None:
            self.countermoves[opponent_move] = our_move

    def get_countermove(self, opponent_move: Optional[tuple]) -> Optional[tuple]:
        """Get the recorded countermove for an opponent's move."""
        if opponent_move is None:
            return None
        return self.countermoves.get(opponent_move)

    def age_history(self):
        """Age history scores (call at start of each search)."""
        # Decay all scores to give recent games more weight
        keys_to_remove = []
        for move in self.history:
            self.history[move] //= self.HISTORY_DECAY
            if self.history[move] == 0:
                keys_to_remove.append(move)

        # Remove zeroed entries to save memory
        for key in keys_to_remove:
            del self.history[key]

    def clear(self):
        """Clear all move ordering data."""
        self.killer_moves.clear()
        self.history.clear()
        self.countermoves.clear()
        self.prev_best = None
        self.last_opponent_move = None

    def clear_killers(self):
        """Clear killer moves only (between searches)."""
        self.killer_moves.clear()


class MoveOrderer:
    """
    Simple move ordering for quick evaluation.
    Used when full MoveGenerator is overkill.
    """

    @staticmethod
    def order_by_center(moves: list) -> list:
        """Order moves by distance from center (closest first)."""
        center = BOARD_SIZE // 2

        def center_distance(move):
            return abs(move[0] - center) + abs(move[1] - center)

        return sorted(moves, key=center_distance)

    @staticmethod
    def order_by_threats(board: Board, moves: list, color: int) -> list:
        """Order moves by immediate threat potential."""
        scored = []

        for move in moves:
            row, col = move
            score = 0

            # Check lines through this position
            for dr, dc in [(0, 1), (1, 0), (1, 1), (1, -1)]:
                my_count = 0
                opp_count = 0

                for i in range(-4, 5):
                    r, c = row + i * dr, col + i * dc
                    if 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE:
                        stone = board.get(r, c)
                        if stone == color:
                            my_count += 1
                        elif stone != EMPTY:
                            opp_count += 1

                score += my_count * 10 + opp_count * 5

            scored.append((score, move))

        scored.sort(reverse=True, key=lambda x: x[0])
        return [move for _, move in scored]
