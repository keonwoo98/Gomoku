"""
Transposition table with Zobrist hashing for position caching.
Stores previously evaluated positions to avoid redundant calculations.
"""

import random
from dataclasses import dataclass
from typing import Optional

from ..game.board import BOARD_SIZE

TOTAL_CELLS = BOARD_SIZE * BOARD_SIZE  # 361

# Entry types for score interpretation
EXACT = 0       # Exact score (PV node)
LOWER_BOUND = 1  # Score is lower bound (fail-high, beta cutoff)
UPPER_BOUND = 2  # Score is upper bound (fail-low, alpha cutoff)


@dataclass
class TTEntry:
    """Transposition table entry."""
    zobrist_hash: int
    depth: int
    score: int
    flag: int  # EXACT, LOWER_BOUND, UPPER_BOUND
    best_move: Optional[tuple]
    age: int  # Search iteration number for replacement


class TranspositionTable:
    """
    Hash table for caching position evaluations.

    Uses Zobrist hashing for fast incremental updates.
    Replacement scheme: prefer deeper searches and newer entries.
    """

    DEFAULT_SIZE_MB = 16

    def __init__(self, size_mb: int = DEFAULT_SIZE_MB):
        """
        Initialize transposition table.

        Args:
            size_mb: Table size in megabytes (default 16MB)
        """
        # Calculate number of entries based on size
        entry_size_estimate = 48  # bytes per entry (approximate)
        self.size = (size_mb * 1024 * 1024) // entry_size_estimate
        self.table: list[Optional[TTEntry]] = [None] * self.size
        self.current_age = 0

        # Initialize Zobrist random numbers (deterministic for reproducibility)
        random.seed(42)
        self.zobrist_black = [random.getrandbits(64) for _ in range(TOTAL_CELLS)]
        self.zobrist_white = [random.getrandbits(64) for _ in range(TOTAL_CELLS)]
        self.zobrist_turn = random.getrandbits(64)  # XOR when black to move

        # Statistics for debugging
        self.hits = 0
        self.misses = 0
        self.stores = 0
        self.overwrites = 0

    def compute_hash(self, board, color: int) -> int:
        """
        Compute Zobrist hash for a board position.

        Optimized: Only iterate over set bits using bit manipulation.

        Args:
            board: Board instance
            color: Side to move (BLACK or WHITE)

        Returns:
            64-bit hash value
        """
        h = 0

        # Hash black stones (only iterate set bits)
        black = board.black
        while black:
            # Get lowest set bit position
            bit = (black & -black).bit_length() - 1
            h ^= self.zobrist_black[bit]
            black &= black - 1  # Clear lowest set bit

        # Hash white stones
        white = board.white
        while white:
            bit = (white & -white).bit_length() - 1
            h ^= self.zobrist_white[bit]
            white &= white - 1

        # Hash side to move (assume BLACK = 1)
        if color == 1:  # BLACK
            h ^= self.zobrist_turn

        return h

    def compute_hash_incremental(self, current_hash: int, row: int, col: int,
                                  color: int, is_place: bool) -> int:
        """
        Incrementally update hash after a move (faster than full recompute).

        Args:
            current_hash: Current position hash
            row, col: Move position
            color: Stone color being placed/removed
            is_place: True if placing stone, False if removing

        Returns:
            Updated hash value
        """
        bit_pos = row * BOARD_SIZE + col

        # XOR is its own inverse, so place and remove are the same operation
        if color == 1:  # BLACK
            current_hash ^= self.zobrist_black[bit_pos]
        else:  # WHITE
            current_hash ^= self.zobrist_white[bit_pos]

        # Toggle side to move
        current_hash ^= self.zobrist_turn

        return current_hash

    def probe(self, zobrist_hash: int, depth: int, alpha: int, beta: int) -> tuple:
        """
        Probe transposition table for a position.

        Args:
            zobrist_hash: Position hash
            depth: Current search depth
            alpha: Alpha bound
            beta: Beta bound

        Returns:
            (hit, score, best_move) where:
            - hit: True if score is usable
            - score: Stored score (valid only if hit is True)
            - best_move: Stored best move (for move ordering)
        """
        idx = zobrist_hash % self.size
        entry = self.table[idx]

        # No entry at this index
        if entry is None:
            self.misses += 1
            return (False, 0, None)

        # Hash collision check (different position)
        if entry.zobrist_hash != zobrist_hash:
            self.misses += 1
            return (False, 0, None)

        # Entry found - check if usable
        self.hits += 1

        # Depth check: only use if stored search was at least as deep
        if entry.depth < depth:
            # Can still use best move for ordering
            return (False, 0, entry.best_move)

        # Check if score is usable based on node type
        if entry.flag == EXACT:
            return (True, entry.score, entry.best_move)
        elif entry.flag == LOWER_BOUND and entry.score >= beta:
            # Stored score was a beta cutoff, score >= stored_score
            return (True, entry.score, entry.best_move)
        elif entry.flag == UPPER_BOUND and entry.score <= alpha:
            # Stored score was below alpha, score <= stored_score
            return (True, entry.score, entry.best_move)

        # Score not usable for cutoff, but return best move
        return (False, 0, entry.best_move)

    def store(self, zobrist_hash: int, depth: int, score: int,
              flag: int, best_move: Optional[tuple]):
        """
        Store position in transposition table.

        Args:
            zobrist_hash: Position hash
            depth: Search depth
            score: Position score
            flag: Score type (EXACT, LOWER_BOUND, UPPER_BOUND)
            best_move: Best move found
        """
        idx = zobrist_hash % self.size
        entry = self.table[idx]

        self.stores += 1

        # Replacement scheme:
        # 1. Always replace empty slots
        # 2. Replace if new search is deeper
        # 3. Replace if entry is from older search iteration
        # 4. Replace if same depth but new entry is EXACT type
        should_replace = (
            entry is None or
            entry.depth < depth or
            entry.age < self.current_age or
            (entry.depth == depth and flag == EXACT and entry.flag != EXACT)
        )

        if should_replace:
            if entry is not None:
                self.overwrites += 1

            self.table[idx] = TTEntry(
                zobrist_hash=zobrist_hash,
                depth=depth,
                score=score,
                flag=flag,
                best_move=best_move,
                age=self.current_age
            )

    def new_search(self):
        """Call at start of each search to age entries."""
        self.current_age += 1

    def clear(self):
        """Clear all entries and reset statistics."""
        self.table = [None] * self.size
        self.current_age = 0
        self.hits = 0
        self.misses = 0
        self.stores = 0
        self.overwrites = 0

    def get_stats(self) -> dict:
        """Get table statistics for debugging."""
        total_probes = self.hits + self.misses
        hit_rate = self.hits / total_probes if total_probes > 0 else 0
        filled = sum(1 for e in self.table if e is not None)
        fill_rate = filled / self.size if self.size > 0 else 0

        return {
            'hits': self.hits,
            'misses': self.misses,
            'hit_rate': f'{hit_rate:.1%}',
            'stores': self.stores,
            'overwrites': self.overwrites,
            'filled': filled,
            'fill_rate': f'{fill_rate:.1%}',
            'size': self.size,
        }

    def __repr__(self) -> str:
        stats = self.get_stats()
        return f"TranspositionTable(hit_rate={stats['hit_rate']}, filled={stats['filled']})"
