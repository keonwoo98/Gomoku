"""Tests for Bitboard implementation."""

import sys
sys.path.insert(0, '.')

from src.game.board import Board, BLACK, WHITE, EMPTY, BOARD_SIZE


class TestBoard:
    """Test cases for Board class."""

    def test_initial_state(self):
        """Board should start empty."""
        board = Board()
        assert board.black == 0
        assert board.white == 0
        assert board.count_stones(BLACK) == 0
        assert board.count_stones(WHITE) == 0

    def test_place_stone(self):
        """Test placing stones."""
        board = Board()

        # Place black stone
        assert board.place_stone(9, 9, BLACK)
        assert board.get(9, 9) == BLACK
        assert board.count_stones(BLACK) == 1

        # Place white stone
        assert board.place_stone(9, 10, WHITE)
        assert board.get(9, 10) == WHITE
        assert board.count_stones(WHITE) == 1

        # Cannot place on occupied cell
        assert not board.place_stone(9, 9, WHITE)

    def test_remove_stone(self):
        """Test removing stones."""
        board = Board()
        board.place_stone(5, 5, BLACK)

        removed = board.remove_stone(5, 5)
        assert removed == BLACK
        assert board.get(5, 5) == EMPTY
        assert board.count_stones(BLACK) == 0

    def test_is_empty(self):
        """Test empty cell detection."""
        board = Board()
        assert board.is_empty(0, 0)

        board.place_stone(0, 0, BLACK)
        assert not board.is_empty(0, 0)

    def test_make_undo_move(self):
        """Test make_move and undo_move."""
        board = Board()

        # Make move
        board.make_move(10, 10, BLACK)
        assert board.get(10, 10) == BLACK

        # Undo move
        result = board.undo_move()
        assert result == (10, 10, BLACK, [])
        assert board.get(10, 10) == EMPTY

    def test_five_in_row_horizontal(self):
        """Test horizontal five detection."""
        board = Board()

        # Place 5 in a row horizontally
        for i in range(5):
            board.place_stone(5, 5 + i, BLACK)

        assert board.has_five_in_row(BLACK)
        assert not board.has_five_in_row(WHITE)

    def test_five_in_row_vertical(self):
        """Test vertical five detection."""
        board = Board()

        for i in range(5):
            board.place_stone(5 + i, 5, WHITE)

        assert board.has_five_in_row(WHITE)
        assert not board.has_five_in_row(BLACK)

    def test_five_in_row_diagonal(self):
        """Test diagonal five detection."""
        board = Board()

        for i in range(5):
            board.place_stone(5 + i, 5 + i, BLACK)

        assert board.has_five_in_row(BLACK)

    def test_copy(self):
        """Test board copy."""
        board = Board()
        board.place_stone(9, 9, BLACK)

        copy = board.copy()
        assert copy.get(9, 9) == BLACK
        assert copy.black == board.black

        # Modify original shouldn't affect copy
        board.place_stone(10, 10, WHITE)
        assert copy.get(10, 10) == EMPTY

    def test_get_adjacent_empty(self):
        """Test getting adjacent empty cells."""
        board = Board()

        # Empty board - should return center
        candidates = board.get_adjacent_empty()
        assert (9, 9) in candidates

        # Place a stone - should get surrounding cells
        board.place_stone(9, 9, BLACK)
        candidates = board.get_adjacent_empty(radius=1)
        assert (9, 9) not in candidates  # Occupied
        assert (8, 8) in candidates
        assert (9, 10) in candidates

    def test_pos_to_bit(self):
        """Test position to bit conversion."""
        assert Board.pos_to_bit(0, 0) == 0
        assert Board.pos_to_bit(0, 1) == 1
        assert Board.pos_to_bit(1, 0) == 19
        assert Board.pos_to_bit(18, 18) == 360

    def test_bit_to_pos(self):
        """Test bit to position conversion."""
        assert Board.bit_to_pos(0) == (0, 0)
        assert Board.bit_to_pos(1) == (0, 1)
        assert Board.bit_to_pos(19) == (1, 0)
        assert Board.bit_to_pos(360) == (18, 18)


if __name__ == "__main__":
    import pytest
    pytest.main([__file__, "-v"])
