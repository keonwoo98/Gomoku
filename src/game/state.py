"""
Game state management for Gomoku.
Tracks current turn, captures, game status, and provides game flow control.
"""

from enum import Enum
from dataclasses import dataclass, field
from typing import Optional
import time

from .board import Board, BLACK, WHITE, EMPTY, BOARD_SIZE
from .rules import Rules


class GameMode(Enum):
    """Game modes."""
    PVP = "pvp"           # Player vs Player (hotseat)
    PVE = "pve"           # Player vs AI
    EVE = "eve"           # AI vs AI (for testing)


class StartingRule(Enum):
    """Starting conditions for game balance."""
    STANDARD = "standard"  # Normal rules, black plays first anywhere
    PRO = "pro"            # First move must be center, black's 2nd must be 3+ away from center
    SWAP = "swap"          # First player places 3 stones, second player chooses color
    SWAP2 = "swap2"        # Extended swap with more options


class AIDifficulty(Enum):
    """AI difficulty levels with corresponding search depths."""
    EASY = ("easy", 5, 0.3)        # Depth 5, 0.3s limit
    MEDIUM = ("medium", 10, 0.4)   # Depth 10, 0.4s limit
    HARD = ("hard", 15, 0.5)       # Depth 15, 0.5s limit
    EXPERT = ("expert", 20, 0.5)   # Depth 20, 0.5s limit (default)

    def __init__(self, label: str, depth: int, time_limit: float):
        self._label = label
        self._depth = depth
        self._time_limit = time_limit

    @property
    def label(self) -> str:
        return self._label

    @property
    def depth(self) -> int:
        return self._depth

    @property
    def time_limit(self) -> float:
        return self._time_limit


class GamePhase(Enum):
    """Game phases for special starting conditions."""
    NORMAL = "normal"              # Normal gameplay
    OPENING_PLACE = "opening"      # Placing opening stones (Swap/Swap2)
    SWAP_CHOICE = "swap_choice"    # Choosing color (Swap)
    SWAP2_CHOICE = "swap2_choice"  # Swap2 decision: pick color or place 2 more
    SWAP2_EXTRA = "swap2_extra"    # Placing 2 extra stones in Swap2
    SWAP2_FINAL = "swap2_final"    # Final color choice in Swap2


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

    def __init__(self, mode: GameMode = GameMode.PVE,
                 starting_rule: StartingRule = StartingRule.STANDARD):
        self.board = Board()
        self.mode = mode
        self.starting_rule = starting_rule
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
        self.last_move_time = {BLACK: 0.0, WHITE: 0.0}  # Per-player last move time

        # Starting rule state
        self.phase = GamePhase.NORMAL
        self.opening_stones: list[tuple] = []  # Stones placed during opening
        self.swap_player = None  # Player who will choose in swap
        self._init_starting_rule()

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

    def _init_starting_rule(self):
        """Initialize game state based on starting rule."""
        if self.starting_rule == StartingRule.STANDARD:
            self.phase = GamePhase.NORMAL
        elif self.starting_rule == StartingRule.PRO:
            self.phase = GamePhase.NORMAL  # Pro uses normal phase with move restrictions
        elif self.starting_rule == StartingRule.SWAP:
            self.phase = GamePhase.OPENING_PLACE
            self.opening_stones = []
        elif self.starting_rule == StartingRule.SWAP2:
            self.phase = GamePhase.OPENING_PLACE
            self.opening_stones = []

    def get_center(self) -> tuple:
        """Get center position of the board."""
        center = BOARD_SIZE // 2
        return (center, center)

    def reset(self, mode: Optional[GameMode] = None,
              starting_rule: Optional[StartingRule] = None):
        """Reset the game to initial state."""
        if mode is not None:
            self.mode = mode
        if starting_rule is not None:
            self.starting_rule = starting_rule

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
        self.last_move_time = {BLACK: 0.0, WHITE: 0.0}
        self.opening_stones = []
        self.swap_player = None
        self._init_starting_rule()
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

        # Handle special phases for Swap/Swap2
        if self.phase == GamePhase.OPENING_PLACE:
            return self._make_opening_move(row, col)
        elif self.phase == GamePhase.SWAP2_EXTRA:
            return self._make_swap2_extra_move(row, col)

        color = self.current_turn

        # Check starting rule restrictions
        if not self._is_valid_for_starting_rule(row, col, color):
            return False

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

        # Store per-player move time
        if thinking_time > 0:
            self.last_move_time[color] = thinking_time

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

    def _is_valid_for_starting_rule(self, row: int, col: int, color: int) -> bool:
        """Check if move is valid according to starting rule restrictions."""
        if self.starting_rule == StartingRule.PRO:
            move_count = self.get_move_count()
            center = self.get_center()

            # First move (Black's 1st) must be at center
            if move_count == 0 and (row, col) != center:
                return False

            # Third move (Black's 2nd) must be at least 3 intersections from center
            if move_count == 2:
                distance = max(abs(row - center[0]), abs(col - center[1]))
                if distance < 3:
                    return False

        return True

    def _make_opening_move(self, row: int, col: int) -> bool:
        """Handle opening stone placement for Swap/Swap2."""
        if not self.board.is_empty(row, col):
            return False

        # Determine which color to place based on opening stone count
        # Pattern: Black, White, Black (3 stones total)
        stone_count = len(self.opening_stones)
        if stone_count == 0:
            color = BLACK
        elif stone_count == 1:
            color = WHITE
        else:
            color = BLACK

        self.board.place_stone(row, col, color)
        self.opening_stones.append((row, col, color))
        self.last_move = (row, col)

        # Record for history
        record = MoveRecord(row=row, col=col, color=color)
        self.move_history.append(record)

        # After 3 stones, move to choice phase
        if len(self.opening_stones) == 3:
            if self.starting_rule == StartingRule.SWAP:
                self.phase = GamePhase.SWAP_CHOICE
                self.swap_player = WHITE  # Second player chooses
            elif self.starting_rule == StartingRule.SWAP2:
                self.phase = GamePhase.SWAP2_CHOICE
                self.swap_player = WHITE

        return True

    def _make_swap2_extra_move(self, row: int, col: int) -> bool:
        """Handle the 2 extra stones in Swap2."""
        if not self.board.is_empty(row, col):
            return False

        extra_count = len(self.opening_stones) - 3
        # Pattern: White, Black (2 more stones)
        color = WHITE if extra_count == 0 else BLACK

        self.board.place_stone(row, col, color)
        self.opening_stones.append((row, col, color))
        self.last_move = (row, col)

        record = MoveRecord(row=row, col=col, color=color)
        self.move_history.append(record)

        # After 5 stones total, first player chooses color
        if len(self.opening_stones) == 5:
            self.phase = GamePhase.SWAP2_FINAL
            self.swap_player = BLACK  # First player now chooses

        return True

    def choose_color(self, chosen_color: int) -> bool:
        """
        Choose color during Swap/Swap2 phase.
        Returns True if successful.
        """
        if self.phase not in [GamePhase.SWAP_CHOICE, GamePhase.SWAP2_FINAL]:
            return False

        # Determine who chose and assign players
        if self.phase == GamePhase.SWAP_CHOICE:
            # Second player (original WHITE) chooses
            if chosen_color == BLACK:
                # Second player takes black, first player gets white
                if self.mode == GameMode.PVE:
                    self.players[BLACK] = Player(BLACK, PlayerType.AI)
                    self.players[WHITE] = Player(WHITE, PlayerType.HUMAN)
                # In PVP, players just swap conceptually
            # If WHITE chosen, keep original assignment
            self.current_turn = WHITE if chosen_color == BLACK else BLACK
        else:  # SWAP2_FINAL
            # First player (original BLACK) chooses
            if chosen_color == WHITE:
                if self.mode == GameMode.PVE:
                    self.players[BLACK] = Player(BLACK, PlayerType.AI)
                    self.players[WHITE] = Player(WHITE, PlayerType.HUMAN)
            self.current_turn = WHITE if chosen_color == BLACK else BLACK

        self.phase = GamePhase.NORMAL
        return True

    def choose_swap2_option(self, option: int) -> bool:
        """
        Choose option during Swap2 choice phase.
        Options: 1=take BLACK, 2=take WHITE, 3=place 2 more stones
        """
        if self.phase != GamePhase.SWAP2_CHOICE:
            return False

        if option == 1:  # Take BLACK
            if self.mode == GameMode.PVE:
                self.players[BLACK] = Player(BLACK, PlayerType.AI)
                self.players[WHITE] = Player(WHITE, PlayerType.HUMAN)
            self.current_turn = WHITE
            self.phase = GamePhase.NORMAL
        elif option == 2:  # Take WHITE
            self.current_turn = BLACK
            self.phase = GamePhase.NORMAL
        elif option == 3:  # Place 2 more stones
            self.phase = GamePhase.SWAP2_EXTRA
        else:
            return False

        return True

    def get_phase_message(self) -> str:
        """Get message describing current phase."""
        if self.phase == GamePhase.OPENING_PLACE:
            count = len(self.opening_stones)
            remaining = 3 - count
            colors = ["Black", "White", "Black"]
            if count < 3:
                return f"Place {colors[count]} stone ({remaining} remaining)"
            return ""
        elif self.phase == GamePhase.SWAP_CHOICE:
            return "Choose color: [B]lack or [W]hite"
        elif self.phase == GamePhase.SWAP2_CHOICE:
            return "Choose: [1]Black [2]White [3]Place 2 more"
        elif self.phase == GamePhase.SWAP2_EXTRA:
            extra = len(self.opening_stones) - 3
            colors = ["White", "Black"]
            return f"Place {colors[extra]} stone ({2-extra} remaining)"
        elif self.phase == GamePhase.SWAP2_FINAL:
            return "Choose color: [B]lack or [W]hite"
        elif self.starting_rule == StartingRule.PRO:
            move_count = self.get_move_count()
            if move_count == 0:
                return "Pro rule: First move must be center"
            elif move_count == 2:
                return "Pro rule: Must be 3+ from center"
        return ""

    def is_in_choice_phase(self) -> bool:
        """Check if game is waiting for a color/option choice."""
        return self.phase in [
            GamePhase.SWAP_CHOICE,
            GamePhase.SWAP2_CHOICE,
            GamePhase.SWAP2_FINAL
        ]

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
            'starting_rule': self.starting_rule.value,
            'phase': self.phase.value,
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
            'phase_message': self.get_phase_message(),
            'in_choice_phase': self.is_in_choice_phase(),
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
