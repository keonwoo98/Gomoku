"""
AI Engine for Gomoku.
Implements Alpha-Beta Pruning with Iterative Deepening.

Phase 2 Enhancements:
- Transposition Table with Zobrist hashing
- Null Move Pruning
- Late Move Reduction (LMR)
- Aspiration Windows
"""

import time
from typing import Optional
from dataclasses import dataclass, field

from ..game.board import Board, BLACK, WHITE, EMPTY
from ..game.rules import Rules
from .heuristic import Heuristic
from .movegen import MoveGenerator
from .transposition import TranspositionTable, EXACT, LOWER_BOUND, UPPER_BOUND


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

    Phase 2 Features:
    - Transposition Table for position caching
    - Null Move Pruning for faster cutoffs
    - Late Move Reduction for deeper searches
    - Aspiration Windows for tighter bounds
    """

    # Score bounds
    INF = 10_000_000
    WIN_SCORE = 1_000_000

    # Default settings
    DEFAULT_TIME_LIMIT = 0.5
    DEFAULT_MAX_DEPTH = 20
    MIN_DEPTH = 5  # Minimum depth to search (must be >= 5 to detect open-four threats)

    # Null Move Pruning settings
    NMP_MIN_DEPTH = 4  # Only apply NMP at depth >= 4 (more conservative)
    NMP_REDUCTION = 2  # Depth reduction for null move

    # Late Move Reduction settings
    LMR_MIN_DEPTH = 4  # Only apply LMR at depth >= 4 (more conservative)
    LMR_MIN_MOVES = 6  # Apply after searching this many moves (was 4)

    # Aspiration Window settings
    ASP_WINDOW = 50  # Initial window size
    ASP_MIN_DEPTH = 100  # Disabled for now (was 4)

    def __init__(self, max_depth: int = DEFAULT_MAX_DEPTH):
        self.heuristic = Heuristic()
        self.move_gen = MoveGenerator(self.heuristic)
        self.tt = TranspositionTable(size_mb=16)

        # Configurable settings
        self.max_depth = max_depth

        # Search state
        self.node_count = 0
        self.start_time = 0.0
        self.time_limit = self.DEFAULT_TIME_LIMIT
        self.should_stop = False

        # Statistics
        self.alpha_cutoffs = 0
        self.beta_cutoffs = 0
        self.null_cutoffs = 0
        self.lmr_reductions = 0
        self.lmr_researches = 0

        # Debug info
        self.debug_info = AIDebugInfo()

        # Best move storage for each depth
        self.best_moves_at_depth = {}
        self.best_score_at_depth = {}

    def set_difficulty(self, max_depth: int, time_limit: float = None):
        """Set AI difficulty parameters."""
        self.max_depth = max_depth
        if time_limit is not None:
            self.time_limit = time_limit

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
        self.null_cutoffs = 0
        self.lmr_reductions = 0
        self.lmr_researches = 0
        self.best_moves_at_depth.clear()
        self.best_score_at_depth.clear()
        self.move_gen.clear_killers()
        self.move_gen.age_history()  # Age history scores
        self.tt.new_search()  # Age TT entries

        # Reset debug info
        self.debug_info = AIDebugInfo()

        opp_color = self._opposite(color)

        # ==================== FORCED MOVE CHECK ====================
        # Check for immediate winning/blocking moves BEFORE search
        # This ensures we NEVER miss critical tactical moves!

        forced_move = self._check_forced_moves(board, color, opp_color, captures)
        if forced_move:
            self.debug_info.thinking_time = time.time() - self.start_time
            self.debug_info.best_move = forced_move
            self.debug_info.best_score = self.WIN_SCORE
            self.debug_info.search_depth = 1
            return forced_move

        # ==================== END FORCED MOVE CHECK ====================

        best_move = None
        best_score = -self.INF
        all_root_scores = []

        # Set last opponent move for countermove heuristic
        if board.move_history:
            last_move = board.move_history[-1]
            self.move_gen.last_opponent_move = (last_move[0], last_move[1])

        # Iterative Deepening with Aspiration Windows
        for depth in range(1, self.max_depth + 1):
            # Check time before starting new depth
            elapsed = time.time() - self.start_time
            if depth > self.MIN_DEPTH and elapsed > time_limit * 0.8:
                break

            # Get previous best for move ordering
            prev_best = self.best_moves_at_depth.get(depth - 1)
            prev_score = self.best_score_at_depth.get(depth - 1, 0)

            # Search at current depth (with aspiration windows for deeper searches)
            try:
                if depth >= self.ASP_MIN_DEPTH and prev_best is not None:
                    # Aspiration window search
                    move, score, pv, root_scores = self._search_with_aspiration(
                        board, color, depth, captures, prev_best, prev_score
                    )
                else:
                    # Full window search
                    move, score, pv, root_scores = self._search_root(
                        board, color, depth, captures, prev_best,
                        -self.INF, self.INF
                    )
            except TimeoutError:
                break

            # Update best if search completed
            if move is not None:
                best_move = move
                best_score = score
                self.best_moves_at_depth[depth] = move
                self.best_score_at_depth[depth] = score
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

    def _search_with_aspiration(self, board: Board, color: int, depth: int,
                                 captures: dict, prev_best: tuple,
                                 prev_score: int) -> tuple:
        """Search with aspiration windows for tighter bounds."""
        window = self.ASP_WINDOW
        alpha = prev_score - window
        beta = prev_score + window

        # Search with narrow window
        move, score, pv, root_scores = self._search_root(
            board, color, depth, captures, prev_best, alpha, beta
        )

        # Re-search if score outside window
        if score is not None:
            if score <= alpha:
                # Fail-low: re-search with lower bound
                move, score, pv, root_scores = self._search_root(
                    board, color, depth, captures, prev_best,
                    -self.INF, score + 1
                )
            elif score >= beta:
                # Fail-high: re-search with upper bound
                move, score, pv, root_scores = self._search_root(
                    board, color, depth, captures, prev_best,
                    score - 1, self.INF
                )

        return (move, score, pv, root_scores)

    def _search_root(self, board: Board, color: int, depth: int,
                     captures: dict, prev_best: Optional[tuple],
                     alpha: int = None, beta: int = None) -> tuple:
        """
        Search from the root position.

        Returns:
            (best_move, best_score, principal_variation, all_root_scores)
        """
        if alpha is None:
            alpha = -self.INF
        if beta is None:
            beta = self.INF

        original_alpha = alpha
        best_move = None
        best_score = -self.INF
        best_pv = []
        all_scores = []

        # Probe TT for move ordering
        zobrist = self.tt.compute_hash(board, color)
        _, _, tt_move = self.tt.probe(zobrist, depth, alpha, beta)

        # Get ordered moves with TT move priority
        moves = self.move_gen.get_moves(board, color, 0, captures, prev_best, tt_move)

        moves_searched = 0
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

            # Search with LMR at root
            child_pv = []
            do_full_search = True

            # Late Move Reduction at root
            if (moves_searched >= self.LMR_MIN_MOVES and
                depth >= self.LMR_MIN_DEPTH and
                len(captured) == 0):  # Not a capture move

                # Reduced depth search
                self.lmr_reductions += 1
                score = -self._alphabeta(
                    board, self._opposite(color), depth - 2,  # Reduced
                    -alpha - 1, -alpha, new_captures, []
                )

                # Re-search if score improves alpha
                do_full_search = (score > alpha)
                if do_full_search:
                    self.lmr_researches += 1

            if do_full_search:
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

                # Update history and countermove for good moves
                self.move_gen.update_history(move, depth)
                if board.move_history:
                    last = board.move_history[-1]
                    self.move_gen.record_countermove((last[0], last[1]), move)

            moves_searched += 1

        # Store in TT
        if best_move is not None:
            flag = EXACT
            if best_score <= original_alpha:
                flag = UPPER_BOUND
            elif best_score >= beta:
                flag = LOWER_BOUND
            self.tt.store(zobrist, depth, best_score, flag, best_move)

        return (best_move, best_score, best_pv, all_scores)

    def _alphabeta(self, board: Board, color: int, depth: int,
                   alpha: int, beta: int, captures: dict,
                   pv: list, allow_null: bool = True) -> int:
        """
        Alpha-Beta search with negamax formulation.

        Enhanced with:
        - Transposition Table lookup/store
        - Null Move Pruning
        - Late Move Reduction

        Args:
            board: Current board state
            color: Color to play
            depth: Remaining depth
            alpha: Alpha bound
            beta: Beta bound
            captures: Capture counts
            pv: Principal variation (output)
            allow_null: Whether null move pruning is allowed

        Returns:
            Score for the position
        """
        self.node_count += 1
        original_alpha = alpha
        opp_color = self._opposite(color)

        # Check time periodically
        if self.node_count % 10000 == 0 and self._should_stop():
            raise TimeoutError()

        # ==================== TRANSPOSITION TABLE PROBE ====================
        zobrist = self.tt.compute_hash(board, color)
        tt_hit, tt_score, tt_move = self.tt.probe(zobrist, depth, alpha, beta)

        if tt_hit:
            return tt_score

        # ==================== TERMINAL NODE CHECKS ====================
        # Check for wins
        if captures.get(color, 0) >= 10:
            return self.WIN_SCORE - (20 - depth)  # Prefer faster wins
        if captures.get(opp_color, 0) >= 10:
            return -self.WIN_SCORE + (20 - depth)

        if board.has_five_in_row(color):
            return self.WIN_SCORE - (20 - depth)
        if board.has_five_in_row(opp_color):
            return -self.WIN_SCORE + (20 - depth)

        # Depth limit reached - evaluate
        if depth <= 0:
            score = self.heuristic.evaluate(board, color, captures)
            self.tt.store(zobrist, 0, score, EXACT, None)
            return score

        # ==================== NULL MOVE PRUNING ====================
        # Skip if: too shallow, or already did null move, or very few stones
        # CRITICAL: Also skip if opponent has threatening patterns!
        opp_has_threat = (self.heuristic._has_closed_four(board, opp_color) or
                         self.heuristic._count_open_threes(board, opp_color) > 0 or
                         self.heuristic._count_closed_threes(board, opp_color) >= 2)

        if (allow_null and
            depth >= self.NMP_MIN_DEPTH and
            board.count_stones(color) + board.count_stones(opp_color) > 4 and
            not opp_has_threat):  # Don't use NMP when opponent has threats!

            # Null move: pass turn to opponent
            null_score = -self._alphabeta(
                board, opp_color, depth - 1 - self.NMP_REDUCTION,
                -beta, -beta + 1, captures, [], allow_null=False
            )

            if null_score >= beta:
                self.null_cutoffs += 1
                return beta  # Null move cutoff

        # ==================== MOVE GENERATION ====================
        prev_best = self.best_moves_at_depth.get(depth)
        moves = self.move_gen.get_moves(board, color, depth, captures, prev_best, tt_move)

        if not moves:
            # No valid moves (rare in Gomoku)
            return 0

        best_score = -self.INF
        best_move = None
        best_child_pv = []
        moves_searched = 0

        for move in moves:
            row, col = move

            # Make move
            captured = Rules.get_captured_positions(board, row, col, color)
            board.make_move(row, col, color, captured)

            new_captures = captures.copy()
            new_captures[color] = new_captures.get(color, 0) + len(captured)

            # ==================== LATE MOVE REDUCTION ====================
            child_pv = []
            do_full_search = True
            is_capture = len(captured) > 0
            is_killer = depth in self.move_gen.killer_moves and move in self.move_gen.killer_moves[depth]

            # LMR: reduce depth for late, quiet moves
            # CRITICAL: Don't apply LMR when opponent has threats!
            if (moves_searched >= self.LMR_MIN_MOVES and
                depth >= self.LMR_MIN_DEPTH and
                not is_capture and
                not is_killer and
                not opp_has_threat):  # Don't use LMR when opponent has threats!

                # Calculate reduction
                reduction = 1
                if depth >= 6 and moves_searched >= 8:
                    reduction = 2

                # Search with reduced depth
                self.lmr_reductions += 1
                score = -self._alphabeta(
                    board, opp_color, depth - 1 - reduction,
                    -alpha - 1, -alpha, new_captures, []
                )

                # Re-search if score improves alpha
                do_full_search = (score > alpha)
                if do_full_search:
                    self.lmr_researches += 1

            if do_full_search:
                # Full depth search
                score = -self._alphabeta(
                    board, opp_color, depth - 1,
                    -beta, -alpha, new_captures, child_pv
                )

            # Undo move
            board.undo_move()

            if score > best_score:
                best_score = score
                best_move = move
                best_child_pv = [move] + child_pv

            if score > alpha:
                alpha = score
                self.move_gen.update_history(move, depth)

            if alpha >= beta:
                # Beta cutoff
                self.move_gen.record_killer(move, depth)
                self.beta_cutoffs += 1
                break

            moves_searched += 1

        # ==================== TRANSPOSITION TABLE STORE ====================
        flag = EXACT
        if best_score <= original_alpha:
            flag = UPPER_BOUND
        elif best_score >= beta:
            flag = LOWER_BOUND

        self.tt.store(zobrist, depth, best_score, flag, best_move)

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
        tt_stats = self.tt.get_stats()
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
            'null_cutoffs': self.null_cutoffs,
            'lmr_reductions': self.lmr_reductions,
            'lmr_researches': self.lmr_researches,
            'tt_hit_rate': tt_stats['hit_rate'],
            'tt_filled': tt_stats['fill_rate'],
        }

    def suggest_move(self, board: Board, color: int, captures: dict,
                     time_limit: float = 0.3) -> tuple:
        """
        Get a suggested move (for human assistance).
        Uses shorter time limit than regular search.
        """
        return self.get_move(board, color, captures, time_limit)

    def _check_forced_moves(self, board: Board, color: int, opp_color: int,
                            captures: dict) -> Optional[tuple]:
        """
        Check for forced moves that MUST be played.
        Returns a move if one is forced, None otherwise.

        Priority:
        1. Safe winning move (5 in a row that can't be broken by capture)
        2. Capture win (10 captures)
        3. Block opponent's winning move
        4. Block opponent's capture win threat
        5. Create unstoppable threat (open-four)
        6. Block opponent's open-four
        7. Block opponent's closed-four
        8. Block opponent's capture threat on our stones
        """
        valid_moves = Rules.get_valid_moves(board, color)
        if not valid_moves:
            return None

        # ==================== PRIORITY 1: SAFE WINNING MOVE ====================
        # Only play 5-in-row if opponent can't break it by capture
        for row, col in valid_moves:
            board.place_stone(row, col, color)
            if board.has_five_in_row(color):
                five_positions = Rules._find_any_five_positions(board, color)
                if five_positions:
                    can_break = Rules.can_break_five(board, five_positions, color)
                    if not can_break:
                        # Safe 5-in-row! Guaranteed win!
                        board.remove_stone(row, col)
                        return (row, col)
            board.remove_stone(row, col)

        # ==================== PRIORITY 2: CAPTURE WIN ====================
        for row, col in valid_moves:
            capture_pos = Rules.get_captured_positions(board, row, col, color)
            if capture_pos:
                new_count = captures.get(color, 0) + len(capture_pos)
                if new_count >= 10:
                    return (row, col)

        # ==================== PRIORITY 3: BLOCK OPPONENT'S CAPTURE WIN ====================
        # If opponent is close to 10 captures, block their capture opportunities
        opp_captures = captures.get(opp_color, 0)
        if opp_captures >= 6:  # Opponent has 6+ captures, danger zone!
            capture_threats = self._find_opponent_capture_threats(board, opp_color)
            if capture_threats:
                # Find moves that block the most capture threats
                best_block = self._find_best_capture_block(board, color, capture_threats, valid_moves)
                if best_block and opp_captures >= 8:
                    # Critical! Must block or opponent wins by capture
                    return best_block

        # ==================== PRIORITY 4: BLOCK OPPONENT'S WIN ====================
        opponent_winning_moves = []
        for row, col in valid_moves:
            board.place_stone(row, col, opp_color)
            if board.has_five_in_row(opp_color):
                opponent_winning_moves.append((row, col))
            board.remove_stone(row, col)

        if len(opponent_winning_moves) == 1:
            return opponent_winning_moves[0]
        elif len(opponent_winning_moves) > 1:
            for move in opponent_winning_moves:
                row, col = move
                board.place_stone(row, col, color)
                if self._creates_open_four(board, row, col, color):
                    board.remove_stone(row, col)
                    return move
                board.remove_stone(row, col)
            return opponent_winning_moves[0]

        # ==================== PRIORITY 5: CREATE OPEN-FOUR ====================
        for row, col in valid_moves:
            board.place_stone(row, col, color)
            if self._creates_open_four(board, row, col, color):
                board.remove_stone(row, col)
                return (row, col)
            board.remove_stone(row, col)

        # ==================== PRIORITY 6: BLOCK OPPONENT'S OPEN-FOUR ====================
        opp_open_four_threats = []
        for row, col in valid_moves:
            board.place_stone(row, col, opp_color)
            if self._creates_open_four(board, row, col, opp_color):
                opp_open_four_threats.append((row, col))
            board.remove_stone(row, col)

        if opp_open_four_threats:
            return opp_open_four_threats[0]

        # ==================== PRIORITY 7: BLOCK OPPONENT'S CLOSED-FOUR ====================
        opp_closed_four_threats = []
        for row, col in valid_moves:
            board.place_stone(row, col, opp_color)
            if self._creates_closed_four(board, row, col, opp_color):
                opp_closed_four_threats.append((row, col))
            board.remove_stone(row, col)

        if len(opp_closed_four_threats) >= 2:
            for move in opp_closed_four_threats:
                row, col = move
                board.place_stone(row, col, color)
                if self._creates_closed_four(board, row, col, color):
                    board.remove_stone(row, col)
                    return move
                board.remove_stone(row, col)
            return opp_closed_four_threats[0]
        elif len(opp_closed_four_threats) == 1:
            return opp_closed_four_threats[0]

        # ==================== PRIORITY 8: BLOCK OPPONENT'S CAPTURE THREAT ====================
        # Protect our stones from being captured (pattern: OPP - OURS - OURS - _)
        capture_threats = self._find_opponent_capture_threats(board, opp_color)
        if capture_threats:
            # Block the most dangerous capture threat
            best_block = self._find_best_capture_block(board, color, capture_threats, valid_moves)
            if best_block:
                return best_block

        # No forced move - proceed with normal search
        return None

    def _find_opponent_capture_threats(self, board: Board, opp_color: int) -> list:
        """
        Find positions where opponent can capture our stones.
        Returns list of (capture_pos, captured_stones) tuples.
        """
        threats = []
        my_color = WHITE if opp_color == BLACK else BLACK

        # Check all empty positions for potential captures
        for r in range(19):
            for c in range(19):
                if board.is_empty(r, c):
                    captures = Rules.get_captured_positions(board, r, c, opp_color)
                    if captures:
                        threats.append(((r, c), captures))
        return threats

    def _find_best_capture_block(self, board: Board, color: int,
                                  capture_threats: list, valid_moves: list) -> Optional[tuple]:
        """
        Find the best move to block opponent's capture threats.
        Returns the blocking move or None.
        """
        if not capture_threats:
            return None

        # Sort by number of stones that would be captured (most dangerous first)
        capture_threats.sort(key=lambda x: len(x[1]), reverse=True)

        # Strategy 1: Play at the capture position ourselves (if valid)
        for capture_pos, captured_stones in capture_threats:
            if capture_pos in valid_moves:
                # Playing here prevents opponent from capturing
                return capture_pos

        # Strategy 2: Move one of the threatened stones
        # (This is complex and may not always be possible)

        return None

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

            # Closed-four: 4 consecutive with at least one end open
            if count == 4 and open_ends >= 1:
                return True

        return False
