"""
Bitboard implementation for Gomoku.
19x19 board represented as 361-bit integers for fast operations.
"""

EMPTY = 0
BLACK = 1
WHITE = 2

BOARD_SIZE = 19
TOTAL_CELLS = BOARD_SIZE * BOARD_SIZE  # 361

# Direction shifts for pattern detection
DIRECTIONS = {
    'horizontal': 1,
    'vertical': BOARD_SIZE,           # 19
    'diagonal_down': BOARD_SIZE + 1,  # 20 (↘)
    'diagonal_up': BOARD_SIZE - 1,    # 18 (↗)
}

# Boundary masks to prevent edge wrapping in bitboard operations
# These masks exclude columns that would wrap during bit shifting
def _create_column_mask(exclude_cols: list) -> int:
    """Create a mask that excludes specified columns."""
    mask = 0
    for row in range(BOARD_SIZE):
        for col in range(BOARD_SIZE):
            if col not in exclude_cols:
                mask |= (1 << (row * BOARD_SIZE + col))
    return mask

# Pre-computed masks for each direction (exclude N rightmost/leftmost cols)
# For horizontal shift: exclude rightmost columns to prevent row wrapping
HORIZONTAL_MASKS = [_create_column_mask(list(range(BOARD_SIZE - i, BOARD_SIZE))) for i in range(6)]
# For diagonal_down (↘): exclude rightmost columns
DIAGONAL_DOWN_MASKS = HORIZONTAL_MASKS
# For diagonal_up (↗): exclude leftmost columns
DIAGONAL_UP_MASKS = [_create_column_mask(list(range(i))) for i in range(6)]


class Board:
    """
    Bitboard representation of Gomoku board.
    Uses two 361-bit integers to track black and white stones.
    """

    def __init__(self):
        self.black = 0  # 361-bit integer for black stones
        self.white = 0  # 361-bit integer for white stones
        self.move_history = []  # Stack of (row, col, color, captured_stones)

    def copy(self):
        """Create a deep copy of the board."""
        new_board = Board()
        new_board.black = self.black
        new_board.white = self.white
        new_board.move_history = self.move_history.copy()
        return new_board

    @staticmethod
    def pos_to_bit(row: int, col: int) -> int:
        """Convert (row, col) to bit position."""
        return row * BOARD_SIZE + col

    @staticmethod
    def bit_to_pos(bit: int) -> tuple:
        """Convert bit position to (row, col)."""
        return (bit // BOARD_SIZE, bit % BOARD_SIZE)

    @staticmethod
    def is_valid_pos(row: int, col: int) -> bool:
        """Check if position is within board bounds."""
        return 0 <= row < BOARD_SIZE and 0 <= col < BOARD_SIZE

    def get(self, row: int, col: int) -> int:
        """Get stone at position. Returns EMPTY, BLACK, or WHITE."""
        if not self.is_valid_pos(row, col):
            return EMPTY

        bit = 1 << self.pos_to_bit(row, col)
        if self.black & bit:
            return BLACK
        if self.white & bit:
            return WHITE
        return EMPTY

    def is_empty(self, row: int, col: int) -> bool:
        """Check if position is empty."""
        return self.get(row, col) == EMPTY

    def place_stone(self, row: int, col: int, color: int) -> bool:
        """
        Place a stone on the board.
        Returns True if successful, False if position is occupied.
        """
        if not self.is_valid_pos(row, col):
            return False
        if not self.is_empty(row, col):
            return False

        bit = 1 << self.pos_to_bit(row, col)
        if color == BLACK:
            self.black |= bit
        else:
            self.white |= bit
        return True

    def remove_stone(self, row: int, col: int) -> int:
        """
        Remove a stone from the board.
        Returns the color of the removed stone.
        """
        bit = 1 << self.pos_to_bit(row, col)

        if self.black & bit:
            self.black &= ~bit
            return BLACK
        if self.white & bit:
            self.white &= ~bit
            return WHITE
        return EMPTY

    def get_occupied(self) -> int:
        """Get bitboard of all occupied positions."""
        return self.black | self.white

    def get_stones(self, color: int) -> int:
        """Get bitboard for specific color."""
        return self.black if color == BLACK else self.white

    def count_stones(self, color: int) -> int:
        """Count number of stones of a color."""
        stones = self.get_stones(color)
        return bin(stones).count('1')

    def make_move(self, row: int, col: int, color: int, captured: list = None):
        """
        Make a move and record it in history.
        captured: list of (row, col) tuples of captured stones
        """
        self.place_stone(row, col, color)
        self.move_history.append((row, col, color, captured or []))

        # Remove captured stones
        if captured:
            for r, c in captured:
                self.remove_stone(r, c)

    def undo_move(self) -> tuple:
        """
        Undo the last move.
        Returns (row, col, color, captured) or None if no history.
        """
        if not self.move_history:
            return None

        row, col, color, captured = self.move_history.pop()
        self.remove_stone(row, col)

        # Restore captured stones
        opp_color = WHITE if color == BLACK else BLACK
        for r, c in captured:
            self.place_stone(r, c, opp_color)

        return (row, col, color, captured)

    def get_adjacent_empty(self, radius: int = 2) -> set:
        """
        Get all empty positions adjacent to existing stones.
        Used for move generation.
        """
        candidates = set()
        occupied = self.get_occupied()

        for bit in range(TOTAL_CELLS):
            if (occupied >> bit) & 1:
                row, col = self.bit_to_pos(bit)
                for dr in range(-radius, radius + 1):
                    for dc in range(-radius, radius + 1):
                        if dr == 0 and dc == 0:
                            continue
                        nr, nc = row + dr, col + dc
                        if self.is_valid_pos(nr, nc) and self.is_empty(nr, nc):
                            candidates.add((nr, nc))

        # If board is empty, return center
        if not candidates:
            center = BOARD_SIZE // 2
            candidates.add((center, center))

        return candidates

    def check_line(self, color: int, direction: str, count: int) -> bool:
        """
        Check if there's a line of 'count' consecutive stones.
        Uses bit shifting for fast detection with boundary masks.
        """
        stones = self.get_stones(color)
        shift = DIRECTIONS[direction]

        # Apply boundary mask based on direction to prevent edge wrapping
        if direction == 'horizontal':
            # Mask out rightmost (count-1) columns
            stones &= HORIZONTAL_MASKS[min(count - 1, 5)]
        elif direction == 'diagonal_down':
            stones &= DIAGONAL_DOWN_MASKS[min(count - 1, 5)]
        elif direction == 'diagonal_up':
            stones &= DIAGONAL_UP_MASKS[min(count - 1, 5)]
        # vertical direction doesn't need masking

        result = stones
        for _ in range(count - 1):
            result &= (result >> shift)

        return result != 0

    def has_five_in_row(self, color: int) -> bool:
        """Check if color has 5 or more in a row."""
        for direction in DIRECTIONS:
            if self.check_line(color, direction, 5):
                return True
        return False

    def get_line_at(self, row: int, col: int, dr: int, dc: int, length: int = 9) -> list:
        """
        Extract a line of cells centered at (row, col) in direction (dr, dc).
        Returns list of (color, row, col) tuples.
        """
        result = []
        half = length // 2

        for i in range(-half, half + 1):
            r, c = row + i * dr, col + i * dc
            if self.is_valid_pos(r, c):
                result.append((self.get(r, c), r, c))
            else:
                result.append((None, r, c))  # Out of bounds marker

        return result

    def __str__(self) -> str:
        """String representation of the board."""
        symbols = {EMPTY: '.', BLACK: 'X', WHITE: 'O'}
        lines = []

        # Column headers
        header = '   ' + ' '.join(f'{i:2d}' for i in range(BOARD_SIZE))
        lines.append(header)

        for row in range(BOARD_SIZE):
            line = f'{row:2d} '
            for col in range(BOARD_SIZE):
                line += f' {symbols[self.get(row, col)]} '
            lines.append(line)

        return '\n'.join(lines)

    def __repr__(self) -> str:
        return f'Board(black={bin(self.black)}, white={bin(self.white)})'
