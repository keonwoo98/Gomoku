"""
AI Engine for Gomoku.
Implements Alpha-Beta Pruning with Iterative Deepening.
"""

import time
from typing import Optional
from dataclasses import dataclass, field

from ..game.board import Board, BLACK, WHITE, EMPTY
from ..game.rules import Rules
from .heuristic import Heuristic
from .movegen import MoveGenerator


@dataclass
class AIDebugInfo:
    """Debug information from AI search."""
    thinking_time: float = 0.0
    search_depth: int = 0
    nodes_evaluated: int = 0
    nodes_per_second: float = 0.0
    best_move: Optional[tuple] = None
    best_score: int = 0
    pv_line: list = field(default_factory=list)
    top_moves: list = field(default_factory=list)
    alpha_cutoffs: int = 0
    beta_cutoffs: int = 0


class AIEngine:
    """
    Gomoku AI using Alpha-Beta Pruning with Iterative Deepening.
    """

    # Score bounds
    INF = 10_000_000
    WIN_SCORE = 1_000_000

    # Default settings
    DEFAULT_TIME_LIMIT = 0.5
    DEFAULT_MAX_DEPTH = 20
    MIN_DEPTH = 4  # Minimum depth to search

    def __init__(self):
        self.heuristic = Heuristic()
        self.move_gen = MoveGenerator(self.heuristic)

        # Search state
        self.node_count = 0
        self.start_time = 0.0
        self.time_limit = self.DEFAULT_TIME_LIMIT
        self.should_stop = False

        # Statistics
        self.alpha_cutoffs = 0
        self.beta_cutoffs = 0

        # Debug info
        self.debug_info = AIDebugInfo()

        # Best move storage for each depth
        self.best_moves_at_depth = {}

    def get_move(self, board: Board, color: int, captures: dict,
                 time_limit: float = DEFAULT_TIME_LIMIT) -> tuple:
        """
        Get the best move for the given position.

        Args:
            board: Current board state
            color: Color to play
            captures: Capture counts {BLACK: n, WHITE: m}
            time_limit: Maximum time to search (seconds)

        Returns:
            (row, col) tuple for the best move
        """
        self.time_limit = time_limit
        self.start_time = time.time()
        self.should_stop = False
        self.node_count = 0
        self.alpha_cutoffs = 0
        self.beta_cutoffs = 0
        self.best_moves_at_depth.clear()
        self.move_gen.clear_killers()

        # Reset debug info
        self.debug_info = AIDebugInfo()

        best_move = None
        best_score = -self.INF
        all_root_scores = []

        # Iterative Deepening
        for depth in range(1, self.DEFAULT_MAX_DEPTH + 1):
            # Check time before starting new depth
            elapsed = time.time() - self.start_time
            if depth > self.MIN_DEPTH and elapsed > time_limit * 0.8:
                break

            # Get previous best for move ordering
            prev_best = self.best_moves_at_depth.get(depth - 1)

            # Search at current depth
            try:
                move, score, pv, root_scores = self._search_root(
                    board, color, depth, captures, prev_best
                )
            except TimeoutError:
                break

            # Update best if search completed
            if move is not None:
                best_move = move
                best_score = score
                self.best_moves_at_depth[depth] = move
                all_root_scores = root_scores

                # Update debug info
                self.debug_info.search_depth = depth
                self.debug_info.best_move = move
                self.debug_info.best_score = score
                self.debug_info.pv_line = pv

                # Early exit on winning move
                if score >= self.WIN_SCORE - 1000:
                    break

        # Finalize debug info
        elapsed = time.time() - self.start_time
        self.debug_info.thinking_time = elapsed
        self.debug_info.nodes_evaluated = self.node_count
        self.debug_info.nodes_per_second = (
            self.node_count / elapsed if elapsed > 0 else 0
        )
        self.debug_info.alpha_cutoffs = self.alpha_cutoffs
        self.debug_info.beta_cutoffs = self.beta_cutoffs

        # Sort and store top moves
        all_root_scores.sort(reverse=True, key=lambda x: x[1])
        self.debug_info.top_moves = all_root_scores[:5]

        # Fallback: if no move found, get any valid move
        if best_move is None:
            valid_moves = Rules.get_valid_moves(board, color)
            if valid_moves:
                best_move = valid_moves[0]

        return best_move

    def _search_root(self, board: Board, color: int, depth: int,
                     captures: dict, prev_best: Optional[tuple]) -> tuple:
        """
        Search from the root position.

        Returns:
            (best_move, best_score, principal_variation, all_root_scores)
        """
        alpha = -self.INF
        beta = self.INF
        best_move = None
        best_score = -self.INF
        best_pv = []
        all_scores = []

        # Get ordered moves
        moves = self.move_gen.get_moves(board, color, 0, captures, prev_best)

        for move in moves:
            # Check time
            if self._should_stop():
                raise TimeoutError()

            row, col = move

            # Make move
            captured = Rules.get_captured_positions(board, row, col, color)
            board.make_move(row, col, color, captured)

            new_captures = captures.copy()
            new_captures[color] = new_captures.get(color, 0) + len(captured)

            # Search
            child_pv = []
            score = -self._alphabeta(
                board, self._opposite(color), depth - 1,
                -beta, -alpha, new_captures, child_pv
            )

            # Undo move
            board.undo_move()

            # Store score for debugging
            all_scores.append((move, score))

            if score > best_score:
                best_score = score
                best_move = move
                best_pv = [move] + child_pv

            if score > alpha:
                alpha = score

                # Update history for good moves
                self.move_gen.update_history(move, depth)

        return (best_move, best_score, best_pv, all_scores)

    def _alphabeta(self, board: Board, color: int, depth: int,
                   alpha: int, beta: int, captures: dict,
                   pv: list) -> int:
        """
        Alpha-Beta search with negamax formulation.

        Args:
            board: Current board state
            color: Color to play
            depth: Remaining depth
            alpha: Alpha bound
            beta: Beta bound
            captures: Capture counts
            pv: Principal variation (output)

        Returns:
            Score for the position
        """
        self.node_count += 1

        # Check time periodically
        if self.node_count % 10000 == 0 and self._should_stop():
            raise TimeoutError()

        # Terminal node checks
        opp_color = self._opposite(color)

        # Check for wins
        if captures.get(color, 0) >= 10:
            return self.WIN_SCORE - (20 - depth)  # Prefer faster wins
        if captures.get(opp_color, 0) >= 10:
            return -self.WIN_SCORE + (20 - depth)

        if board.has_five_in_row(color):
            return self.WIN_SCORE - (20 - depth)
        if board.has_five_in_row(opp_color):
            return -self.WIN_SCORE + (20 - depth)

        # Depth limit reached
        if depth <= 0:
            return self.heuristic.evaluate(board, color, captures)

        # Get moves
        prev_best = self.best_moves_at_depth.get(depth)
        moves = self.move_gen.get_moves(board, color, depth, captures, prev_best)

        if not moves:
            # No valid moves (rare in Gomoku)
            return 0

        best_score = -self.INF
        best_child_pv = []

        for move in moves:
            row, col = move

            # Make move
            captured = Rules.get_captured_positions(board, row, col, color)
            board.make_move(row, col, color, captured)

            new_captures = captures.copy()
            new_captures[color] = new_captures.get(color, 0) + len(captured)

            # Recurse
            child_pv = []
            score = -self._alphabeta(
                board, opp_color, depth - 1,
                -beta, -alpha, new_captures, child_pv
            )

            # Undo move
            board.undo_move()

            if score > best_score:
                best_score = score
                best_child_pv = [move] + child_pv

            if score > alpha:
                alpha = score

            if alpha >= beta:
                # Record killer move
                self.move_gen.record_killer(move, depth)
                self.beta_cutoffs += 1
                break

        pv.clear()
        pv.extend(best_child_pv)

        return best_score

    def _should_stop(self) -> bool:
        """Check if search should stop due to time limit."""
        if self.should_stop:
            return True
        elapsed = time.time() - self.start_time
        if elapsed > self.time_limit * 0.95:
            self.should_stop = True
            return True
        return False

    @staticmethod
    def _opposite(color: int) -> int:
        """Get opposite color."""
        return WHITE if color == BLACK else BLACK

    def get_debug_info(self) -> dict:
        """Get debug information as dictionary."""
        return {
            'thinking_time': self.debug_info.thinking_time,
            'search_depth': self.debug_info.search_depth,
            'nodes_evaluated': self.debug_info.nodes_evaluated,
            'nodes_per_second': self.debug_info.nodes_per_second,
            'best_move': self.debug_info.best_move,
            'best_score': self.debug_info.best_score,
            'pv_line': self.debug_info.pv_line,
            'top_moves': self.debug_info.top_moves,
            'alpha_cutoffs': self.debug_info.alpha_cutoffs,
            'beta_cutoffs': self.debug_info.beta_cutoffs,
        }

    def suggest_move(self, board: Board, color: int, captures: dict,
                     time_limit: float = 0.3) -> tuple:
        """
        Get a suggested move (for human assistance).
        Uses shorter time limit than regular search.
        """
        return self.get_move(board, color, captures, time_limit)
