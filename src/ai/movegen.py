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

    def __init__(self, heuristic: Heuristic):
        self.heuristic = heuristic

        # Killer moves: moves that caused cutoffs at each depth
        self.killer_moves = {}  # depth -> [move1, move2]
        self.max_killers = 2

        # History heuristic: accumulated scores for moves
        self.history = {}  # move -> score

        # Previous best move from iterative deepening
        self.prev_best = None

    def get_moves(self, board: Board, color: int, depth: int,
                  captures: dict, prev_best: Optional[tuple] = None) -> list:
        """
        Get ordered list of candidate moves.

        Args:
            board: Current board state
            color: Color to generate moves for
            depth: Current search depth
            captures: Current capture counts
            prev_best: Best move from previous iteration

        Returns:
            List of (row, col) tuples, ordered by expected quality
        """
        # Get all candidate positions (near existing stones)
        candidates = board.get_adjacent_empty(radius=2)

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
            score = self._score_move(board, move, color, depth, captures, prev_best)
            scored_moves.append((score, move))

        # Sort by score (highest first)
        scored_moves.sort(reverse=True, key=lambda x: x[0])

        return [move for score, move in scored_moves]

    def _score_move(self, board: Board, move: tuple, color: int,
                    depth: int, captures: dict, prev_best: Optional[tuple]) -> int:
        """Score a move for ordering purposes."""
        row, col = move
        score = 0

        # 1. Previous iteration's best move (highest priority)
        if prev_best and move == prev_best:
            score += 10_000_000

        # 2. Winning move
        board.place_stone(row, col, color)
        if board.has_five_in_row(color):
            board.remove_stone(row, col)
            return 9_000_000
        board.remove_stone(row, col)

        # 3. Block opponent's winning move
        opp_color = WHITE if color == BLACK else BLACK
        board.place_stone(row, col, opp_color)
        if board.has_five_in_row(opp_color):
            score += 8_000_000
        board.remove_stone(row, col)

        # 4. Capture moves
        capture_positions = Rules.get_captured_positions(board, row, col, color)
        score += len(capture_positions) * 100_000

        # 5. Capture win check
        if captures.get(color, 0) + len(capture_positions) >= 10:
            return 9_500_000

        # 6. Killer moves
        if depth in self.killer_moves and move in self.killer_moves[depth]:
            score += 50_000

        # 7. History heuristic
        score += self.history.get(move, 0)

        # 8. Quick heuristic evaluation
        score += self.heuristic.evaluate_move(board, row, col, color, captures)

        return score

    def record_killer(self, move: tuple, depth: int):
        """Record a killer move at a depth."""
        if depth not in self.killer_moves:
            self.killer_moves[depth] = []

        killers = self.killer_moves[depth]
        if move not in killers:
            killers.insert(0, move)
            if len(killers) > self.max_killers:
                killers.pop()

    def update_history(self, move: tuple, depth: int):
        """Update history score for a move."""
        if move not in self.history:
            self.history[move] = 0
        self.history[move] += depth * depth

    def clear(self):
        """Clear all move ordering data."""
        self.killer_moves.clear()
        self.history.clear()
        self.prev_best = None

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
