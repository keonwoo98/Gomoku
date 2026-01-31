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
        # Get all candidate positions (near existing stones)
        # Use smaller radius at deeper depths for pruning
        radius = 2 if depth >= 3 else 1
        candidates = board.get_adjacent_empty(radius=radius)

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
        Ultra-optimized: Minimal computation for fast move ordering.
        """
        row, col = move

        # Priority levels (return immediately for high-priority moves)
        # 1. TT move
        if tt_move and move == tt_move:
            return 20_000_000

        # 2. Previous iteration's best
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
