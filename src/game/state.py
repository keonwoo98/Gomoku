"""
Game state management for Gomoku.
Tracks current turn, captures, game status, and provides game flow control.
"""

from enum import Enum
from dataclasses import dataclass, field
from typing import Optional
import time

from .board import Board, BLACK, WHITE, EMPTY
from .rules import Rules


class GameMode(Enum):
    """Game modes."""
    PVP = "pvp"           # Player vs Player (hotseat)
    PVE = "pve"           # Player vs AI
    EVE = "eve"           # AI vs AI (for testing)


class PlayerType(Enum):
    """Player types."""
    HUMAN = "human"
    AI = "ai"


@dataclass
class Player:
    """Player information."""
    color: int
    player_type: PlayerType
    name: str = ""

    def __post_init__(self):
        if not self.name:
            color_name = "Black" if self.color == BLACK else "White"
            type_name = "Human" if self.player_type == PlayerType.HUMAN else "AI"
            self.name = f"{color_name} ({type_name})"


@dataclass
class MoveRecord:
    """Record of a single move."""
    row: int
    col: int
    color: int
    captured: list = field(default_factory=list)
    thinking_time: float = 0.0


class GameState:
    """
    Manages the complete state of a Gomoku game.
    """

    def __init__(self, mode: GameMode = GameMode.PVE):
        self.board = Board()
        self.mode = mode
        self.current_turn = BLACK  # Black always starts
        self.captures = {BLACK: 0, WHITE: 0}
        self.move_history: list[MoveRecord] = []
        self.winner = EMPTY
        self.is_game_over = False
        self.last_move: Optional[tuple] = None

        # AI timing
        self.ai_thinking = False
        self.ai_start_time = 0.0
        self.last_ai_time = 0.0

        # Players setup based on mode
        self._setup_players(mode)

    def _setup_players(self, mode: GameMode):
        """Setup players based on game mode."""
        if mode == GameMode.PVP:
            self.players = {
                BLACK: Player(BLACK, PlayerType.HUMAN),
                WHITE: Player(WHITE, PlayerType.HUMAN),
            }
        elif mode == GameMode.PVE:
            self.players = {
                BLACK: Player(BLACK, PlayerType.HUMAN),
                WHITE: Player(WHITE, PlayerType.AI),
            }
        else:  # EVE
            self.players = {
                BLACK: Player(BLACK, PlayerType.AI),
                WHITE: Player(WHITE, PlayerType.AI),
            }

    def reset(self, mode: Optional[GameMode] = None):
        """Reset the game to initial state."""
        if mode is not None:
            self.mode = mode

        self.board = Board()
        self.current_turn = BLACK
        self.captures = {BLACK: 0, WHITE: 0}
        self.move_history = []
        self.winner = EMPTY
        self.is_game_over = False
        self.last_move = None
        self.ai_thinking = False
        self.ai_start_time = 0.0
        self.last_ai_time = 0.0
        self._setup_players(self.mode)

    def get_current_player(self) -> Player:
        """Get the current player."""
        return self.players[self.current_turn]

    def is_ai_turn(self) -> bool:
        """Check if it's AI's turn."""
        return (not self.is_game_over and
                self.get_current_player().player_type == PlayerType.AI)

    def is_human_turn(self) -> bool:
        """Check if it's human's turn."""
        return (not self.is_game_over and
                self.get_current_player().player_type == PlayerType.HUMAN)

    def make_move(self, row: int, col: int, thinking_time: float = 0.0) -> bool:
        """
        Attempt to make a move at (row, col).
        Returns True if move was successful.
        """
        if self.is_game_over:
            return False

        color = self.current_turn

        # Validate move
        if not Rules.is_valid_move(self.board, row, col, color):
            return False

        # Check for captures
        captured_positions = Rules.get_captured_positions(self.board, row, col, color)

        # Make the move
        self.board.make_move(row, col, color, captured_positions)

        # Update capture count
        self.captures[color] += len(captured_positions)

        # Record the move
        record = MoveRecord(
            row=row,
            col=col,
            color=color,
            captured=captured_positions,
            thinking_time=thinking_time
        )
        self.move_history.append(record)
        self.last_move = (row, col)

        # Check for winner
        self.winner = Rules.check_winner(
            self.board, row, col, color, self.captures
        )

        if self.winner != EMPTY:
            self.is_game_over = True

        # Switch turn
        if not self.is_game_over:
            self.current_turn = WHITE if color == BLACK else BLACK

        return True

    def undo_move(self) -> bool:
        """Undo the last move."""
        if not self.move_history:
            return False

        # Restore board state
        self.board.undo_move()

        # Remove from history
        record = self.move_history.pop()

        # Restore capture count
        self.captures[record.color] -= len(record.captured)

        # Update game state
        self.is_game_over = False
        self.winner = EMPTY
        self.current_turn = record.color

        # Update last move
        if self.move_history:
            last = self.move_history[-1]
            self.last_move = (last.row, last.col)
        else:
            self.last_move = None

        return True

    def get_valid_moves(self) -> list:
        """Get all valid moves for current player."""
        return Rules.get_valid_moves(self.board, self.current_turn)

    def start_ai_timer(self):
        """Start timing AI computation."""
        self.ai_thinking = True
        self.ai_start_time = time.time()

    def stop_ai_timer(self):
        """Stop timing AI computation."""
        self.ai_thinking = False
        self.last_ai_time = time.time() - self.ai_start_time

    def get_ai_elapsed_time(self) -> float:
        """Get elapsed time for current AI computation."""
        if self.ai_thinking:
            return time.time() - self.ai_start_time
        return self.last_ai_time

    def get_move_count(self) -> int:
        """Get total number of moves made."""
        return len(self.move_history)

    def get_game_info(self) -> dict:
        """Get current game information."""
        return {
            'mode': self.mode.value,
            'turn': 'Black' if self.current_turn == BLACK else 'White',
            'move_count': self.get_move_count(),
            'captures': {
                'black': self.captures[BLACK],
                'white': self.captures[WHITE],
            },
            'is_game_over': self.is_game_over,
            'winner': {EMPTY: None, BLACK: 'Black', WHITE: 'White'}.get(self.winner),
            'last_move': self.last_move,
            'last_ai_time': self.last_ai_time,
        }

    def __str__(self) -> str:
        info = self.get_game_info()
        lines = [
            f"Mode: {info['mode']}",
            f"Turn: {info['turn']} (Move #{info['move_count'] + 1})",
            f"Captures - Black: {info['captures']['black']}, White: {info['captures']['white']}",
        ]
        if info['is_game_over']:
            lines.append(f"Game Over! Winner: {info['winner'] or 'Draw'}")
        return '\n'.join(lines)
