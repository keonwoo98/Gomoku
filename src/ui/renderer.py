"""
Pygame renderer for Gomoku.
Handles all visual rendering of the game.
"""

import pygame
import time
from typing import Optional

from ..game.board import BOARD_SIZE, BLACK, WHITE, EMPTY
from ..game.state import GameState, GameMode, StartingRule, GamePhase, AIDifficulty

# Window settings
WINDOW_WIDTH = 1000
WINDOW_HEIGHT = 720  # Compact - matches board height

# Board settings
BOARD_MARGIN = 40
BOARD_AREA_SIZE = 640
CELL_SIZE = BOARD_AREA_SIZE // (BOARD_SIZE + 1)
STONE_RADIUS = CELL_SIZE // 2 - 2

# Panel settings
PANEL_X = BOARD_MARGIN + BOARD_AREA_SIZE + 20
PANEL_WIDTH = WINDOW_WIDTH - PANEL_X - 20

# Colors
COLOR_BG = (40, 44, 52)
COLOR_BOARD = (220, 179, 92)
COLOR_LINE = (50, 40, 30)
COLOR_BLACK_STONE = (20, 20, 20)
COLOR_WHITE_STONE = (240, 240, 240)
COLOR_LAST_MOVE = (255, 80, 80)
COLOR_VALID_MOVE = (100, 200, 100, 128)
COLOR_TEXT = (220, 220, 220)
COLOR_TEXT_DARK = (40, 40, 40)
COLOR_PANEL_BG = (50, 54, 62)
COLOR_BUTTON = (70, 80, 90)
COLOR_BUTTON_HOVER = (90, 100, 110)
COLOR_HIGHLIGHT = (255, 200, 100)
COLOR_CAPTURE_BAR = (100, 180, 255)
COLOR_CAPTURE_BG = (40, 45, 55)
COLOR_WIN_HIGHLIGHT = (255, 215, 0)
COLOR_CAPTURE_FLASH = (255, 100, 100)
COLOR_WIN_LINE = (50, 255, 50)

# Star points (hoshi) positions
STAR_POINTS = [
    (3, 3), (3, 9), (3, 15),
    (9, 3), (9, 9), (9, 15),
    (15, 3), (15, 9), (15, 15),
]


class Renderer:
    """Handles rendering of the Gomoku game."""

    def __init__(self):
        pygame.init()
        pygame.display.set_caption("Gomoku")

        self.screen = pygame.display.set_mode((WINDOW_WIDTH, WINDOW_HEIGHT))
        self.clock = pygame.time.Clock()

        # Fonts
        self.font_large = pygame.font.Font(None, 48)
        self.font_xlarge = pygame.font.Font(None, 72)
        self.font_medium = pygame.font.Font(None, 32)
        self.font_small = pygame.font.Font(None, 24)

        # Hover state
        self.hover_pos: Optional[tuple] = None
        self.show_valid_moves = False

        # Buttons
        self.buttons = {}
        self._setup_buttons()

        # Animation state
        self.capture_flash_time = 0
        self.capture_flash_positions = []
        self.capture_flash_color = BLACK
        self.last_capture_count = {BLACK: 0, WHITE: 0}

        # Win animation
        self.win_line_positions = []
        self.win_animation_start = 0

        # Overlay state
        self.show_help_overlay = False
        self.show_rules_overlay = False

        # Error message state
        self.error_message = ""
        self.error_message_time = 0

    def _setup_buttons(self):
        """Setup button positions and sizes (will be positioned dynamically)."""
        self.button_width = 120
        self.button_height = 38
        self.buttons = {}

    def board_to_screen(self, row: int, col: int) -> tuple:
        """Convert board coordinates to screen coordinates."""
        x = BOARD_MARGIN + CELL_SIZE + col * CELL_SIZE
        y = BOARD_MARGIN + CELL_SIZE + row * CELL_SIZE
        return (x, y)

    def screen_to_board(self, x: int, y: int) -> Optional[tuple]:
        """Convert screen coordinates to board coordinates."""
        # Calculate board position
        col = round((x - BOARD_MARGIN - CELL_SIZE) / CELL_SIZE)
        row = round((y - BOARD_MARGIN - CELL_SIZE) / CELL_SIZE)

        # Check if within board bounds
        if 0 <= row < BOARD_SIZE and 0 <= col < BOARD_SIZE:
            # Check if close enough to intersection
            bx, by = self.board_to_screen(row, col)
            dist = ((x - bx) ** 2 + (y - by) ** 2) ** 0.5
            if dist < CELL_SIZE * 0.6:
                return (row, col)

        return None

    def trigger_capture_flash(self, positions: list, color: int):
        """Trigger capture flash animation."""
        if positions:
            self.capture_flash_positions = positions
            self.capture_flash_color = color
            self.capture_flash_time = time.time()

    def set_win_line(self, positions: list):
        """Set the winning line positions for animation."""
        self.win_line_positions = positions
        self.win_animation_start = time.time()

    def show_error(self, message: str):
        """Show an error message temporarily."""
        self.error_message = message
        self.error_message_time = time.time()

    def reset_animations(self):
        """Reset all animation states."""
        self.capture_flash_time = 0
        self.capture_flash_positions = []
        self.win_line_positions = []
        self.win_animation_start = 0
        self.last_capture_count = {BLACK: 0, WHITE: 0}

    def render(self, state: GameState, suggested_move: Optional[tuple] = None,
               debug_info: Optional[dict] = None, show_debug: bool = False,
               difficulty: AIDifficulty = None):
        """Render the complete game state."""
        # Check for new captures
        self._check_capture_animation(state)

        # Background
        self.screen.fill(COLOR_BG)

        # Board
        self._render_board(state, suggested_move)

        # Side panel
        self._render_panel(state, difficulty)

        # Debug panel (if enabled)
        if show_debug and debug_info:
            self._render_debug_panel(debug_info)

        # Game over result shown in panel instead of overlay

        # Help overlay
        if self.show_help_overlay:
            self._render_help_overlay()

        # Rules overlay
        if self.show_rules_overlay:
            self._render_rules_overlay(state)

        # Error message (temporary, fades after 2 seconds)
        if self.error_message and time.time() - self.error_message_time < 2.0:
            elapsed = time.time() - self.error_message_time
            alpha = int(255 * (1 - elapsed / 2.0))

            # Red error box at bottom of board
            error_box = pygame.Surface((400, 40), pygame.SRCALPHA)
            error_box.fill((180, 50, 50, min(200, alpha)))
            box_x = BOARD_MARGIN + (BOARD_AREA_SIZE - 400) // 2
            box_y = BOARD_MARGIN + BOARD_AREA_SIZE - 50
            self.screen.blit(error_box, (box_x, box_y))

            # Error text
            error_text = self.font_medium.render(self.error_message, True, (255, 255, 255))
            text_x = box_x + (400 - error_text.get_width()) // 2
            self.screen.blit(error_text, (text_x, box_y + 8))

        pygame.display.flip()

    def _check_capture_animation(self, state: GameState):
        """Check if captures occurred and trigger animation."""
        for color in [BLACK, WHITE]:
            current = state.captures.get(color, 0)
            if current > self.last_capture_count.get(color, 0):
                # Captures increased - but we need positions from state
                # This will be called from main.py with actual positions
                pass
            self.last_capture_count[color] = current

    def _render_board(self, state: GameState, suggested_move: Optional[tuple] = None):
        """Render the game board."""
        # Board background
        board_rect = pygame.Rect(
            BOARD_MARGIN, BOARD_MARGIN,
            BOARD_AREA_SIZE, BOARD_AREA_SIZE
        )
        pygame.draw.rect(self.screen, COLOR_BOARD, board_rect)
        pygame.draw.rect(self.screen, COLOR_LINE, board_rect, 2)

        # Grid lines
        for i in range(BOARD_SIZE):
            # Vertical lines
            x = BOARD_MARGIN + CELL_SIZE + i * CELL_SIZE
            y1 = BOARD_MARGIN + CELL_SIZE
            y2 = BOARD_MARGIN + CELL_SIZE + (BOARD_SIZE - 1) * CELL_SIZE
            pygame.draw.line(self.screen, COLOR_LINE, (x, y1), (x, y2), 1)

            # Horizontal lines
            y = BOARD_MARGIN + CELL_SIZE + i * CELL_SIZE
            x1 = BOARD_MARGIN + CELL_SIZE
            x2 = BOARD_MARGIN + CELL_SIZE + (BOARD_SIZE - 1) * CELL_SIZE
            pygame.draw.line(self.screen, COLOR_LINE, (x1, y), (x2, y), 1)

        # Star points
        for row, col in STAR_POINTS:
            x, y = self.board_to_screen(row, col)
            pygame.draw.circle(self.screen, COLOR_LINE, (x, y), 4)

        # Pro rule visual hints
        if state.starting_rule == StartingRule.PRO and not state.is_game_over:
            center = state.get_center()
            cx, cy = self.board_to_screen(center[0], center[1])
            move_count = state.get_move_count()

            if move_count == 0:
                # First move must be center - highlight it
                pygame.draw.circle(self.screen, COLOR_HIGHLIGHT, (cx, cy), STONE_RADIUS + 2, 3)
                # Draw "1st" text
                hint = self.font_small.render("1st", True, COLOR_HIGHLIGHT)
                self.screen.blit(hint, (cx - hint.get_width() // 2, cy - STONE_RADIUS - 18))

            elif move_count == 2:
                # Black's 2nd move must be 3+ from center - show forbidden zone
                # Draw semi-transparent red zone
                s = pygame.Surface((BOARD_AREA_SIZE, BOARD_AREA_SIZE), pygame.SRCALPHA)
                for dr in range(-2, 3):
                    for dc in range(-2, 3):
                        r, c = center[0] + dr, center[1] + dc
                        if 0 <= r < BOARD_SIZE and 0 <= c < BOARD_SIZE:
                            if state.board.is_empty(r, c):
                                sx, sy = self.board_to_screen(r, c)
                                pygame.draw.circle(self.screen, (255, 100, 100, 100),
                                                 (sx, sy), STONE_RADIUS - 2)
                                pygame.draw.line(self.screen, (255, 80, 80),
                                               (sx - 6, sy - 6), (sx + 6, sy + 6), 2)
                                pygame.draw.line(self.screen, (255, 80, 80),
                                               (sx + 6, sy - 6), (sx - 6, sy + 6), 2)

        # Coordinate labels
        for i in range(BOARD_SIZE):
            # Column labels (A-S)
            label = chr(ord('A') + i)
            x, _ = self.board_to_screen(0, i)
            text = self.font_small.render(label, True, COLOR_TEXT_DARK)
            self.screen.blit(text, (x - text.get_width() // 2, BOARD_MARGIN + 5))

            # Row labels (1-19)
            label = str(BOARD_SIZE - i)
            _, y = self.board_to_screen(i, 0)
            text = self.font_small.render(label, True, COLOR_TEXT_DARK)
            self.screen.blit(text, (BOARD_MARGIN + 5, y - text.get_height() // 2))

        # Valid moves (if showing)
        if self.show_valid_moves and not state.is_game_over:
            valid_moves = state.get_valid_moves()
            for row, col in valid_moves:
                x, y = self.board_to_screen(row, col)
                s = pygame.Surface((CELL_SIZE, CELL_SIZE), pygame.SRCALPHA)
                pygame.draw.circle(s, COLOR_VALID_MOVE, (CELL_SIZE // 2, CELL_SIZE // 2), 8)
                self.screen.blit(s, (x - CELL_SIZE // 2, y - CELL_SIZE // 2))

        # Capture flash animation (show where captures happened)
        if self.capture_flash_positions and time.time() - self.capture_flash_time < 1.0:
            elapsed = time.time() - self.capture_flash_time
            alpha = int(255 * (1 - elapsed))
            pulse = abs(int(20 * (1 - elapsed) * (1 + 0.5 * (elapsed * 10 % 2))))

            for row, col in self.capture_flash_positions:
                x, y = self.board_to_screen(row, col)
                # Draw expanding ring
                radius = STONE_RADIUS + int(15 * elapsed)
                s = pygame.Surface((radius * 2 + 20, radius * 2 + 20), pygame.SRCALPHA)
                pygame.draw.circle(s, (*COLOR_CAPTURE_FLASH[:3], alpha),
                                 (radius + 10, radius + 10), radius, 4)
                self.screen.blit(s, (x - radius - 10, y - radius - 10))

                # Draw X mark
                size = 10 + pulse
                pygame.draw.line(self.screen, (*COLOR_CAPTURE_FLASH[:3], alpha),
                               (x - size, y - size), (x + size, y + size), 3)
                pygame.draw.line(self.screen, (*COLOR_CAPTURE_FLASH[:3], alpha),
                               (x + size, y - size), (x - size, y + size), 3)

        # Winning line highlight
        if self.win_line_positions and state.is_game_over:
            elapsed = time.time() - self.win_animation_start
            pulse = 0.5 + 0.5 * abs((elapsed * 3) % 2 - 1)

            # Draw connecting line
            if len(self.win_line_positions) >= 2:
                points = [self.board_to_screen(r, c) for r, c in self.win_line_positions]
                # Sort points to draw line properly
                points.sort()
                pygame.draw.line(self.screen, COLOR_WIN_LINE,
                               points[0], points[-1], 5)

            # Highlight winning stones
            for row, col in self.win_line_positions:
                x, y = self.board_to_screen(row, col)
                radius = int(STONE_RADIUS + 5 * pulse)
                pygame.draw.circle(self.screen, COLOR_WIN_HIGHLIGHT, (x, y), radius, 4)

        # Stones
        for row in range(BOARD_SIZE):
            for col in range(BOARD_SIZE):
                stone = state.board.get(row, col)
                if stone != EMPTY:
                    self._render_stone(row, col, stone)

        # Last move marker
        if state.last_move:
            row, col = state.last_move
            x, y = self.board_to_screen(row, col)
            pygame.draw.circle(self.screen, COLOR_LAST_MOVE, (x, y), 5)

        # Suggested move
        if suggested_move:
            row, col = suggested_move
            x, y = self.board_to_screen(row, col)
            pygame.draw.circle(self.screen, COLOR_HIGHLIGHT, (x, y), STONE_RADIUS, 3)

        # Hover indicator
        if self.hover_pos and not state.is_game_over and state.is_human_turn():
            row, col = self.hover_pos
            if state.board.is_empty(row, col):
                x, y = self.board_to_screen(row, col)
                color = COLOR_BLACK_STONE if state.current_turn == BLACK else COLOR_WHITE_STONE
                s = pygame.Surface((STONE_RADIUS * 2, STONE_RADIUS * 2), pygame.SRCALPHA)
                pygame.draw.circle(s, (*color[:3], 128), (STONE_RADIUS, STONE_RADIUS), STONE_RADIUS)
                self.screen.blit(s, (x - STONE_RADIUS, y - STONE_RADIUS))

    def _render_stone(self, row: int, col: int, color: int):
        """Render a single stone."""
        x, y = self.board_to_screen(row, col)

        # Shadow
        shadow_offset = 2
        pygame.draw.circle(self.screen, (30, 30, 30),
                          (x + shadow_offset, y + shadow_offset), STONE_RADIUS)

        # Stone
        stone_color = COLOR_BLACK_STONE if color == BLACK else COLOR_WHITE_STONE
        pygame.draw.circle(self.screen, stone_color, (x, y), STONE_RADIUS)

        # Highlight (for white stones)
        if color == WHITE:
            highlight_pos = (x - STONE_RADIUS // 3, y - STONE_RADIUS // 3)
            pygame.draw.circle(self.screen, (255, 255, 255), highlight_pos, 3)

    def _render_panel(self, state: GameState, difficulty: AIDifficulty = None):
        """Render the side panel with game info (minimal design)."""
        # Panel background (matches board height)
        panel_rect = pygame.Rect(PANEL_X, BOARD_MARGIN, PANEL_WIDTH, BOARD_AREA_SIZE)
        pygame.draw.rect(self.screen, COLOR_PANEL_BG, panel_rect, border_radius=10)

        y_offset = BOARD_MARGIN + 20

        # Title
        title = self.font_large.render("GOMOKU", True, COLOR_TEXT)
        self.screen.blit(title, (PANEL_X + 20, y_offset))
        y_offset += 50

        # Compact info line: Mode • Difficulty • Rule
        mode_short = {
            GameMode.PVP: "PvP",
            GameMode.PVE: "PvE",
            GameMode.EVE: "EvE",
        }.get(state.mode, "?")

        rule_short = {
            StartingRule.STANDARD: "Standard",
            StartingRule.PRO: "Pro",
            StartingRule.SWAP: "Swap",
            StartingRule.SWAP2: "Swap2",
        }.get(state.starting_rule, "?")

        if state.mode in [GameMode.PVE, GameMode.EVE] and difficulty:
            diff_short = difficulty.label.capitalize()
            info_text = f"{mode_short}  •  {diff_short}  •  {rule_short}"
        else:
            info_text = f"{mode_short}  •  {rule_short}"

        info_label = self.font_small.render(info_text, True, (150, 150, 150))
        self.screen.blit(info_label, (PANEL_X + 20, y_offset))
        y_offset += 30

        # Phase message (only when needed)
        phase_msg = state.get_phase_message()
        if phase_msg:
            msg_box = pygame.Rect(PANEL_X + 15, y_offset, PANEL_WIDTH - 30, 32)
            pygame.draw.rect(self.screen, (60, 55, 40), msg_box, border_radius=6)
            pygame.draw.rect(self.screen, COLOR_HIGHLIGHT, msg_box, 2, border_radius=6)

            phase_text = self.font_small.render(phase_msg, True, COLOR_HIGHLIGHT)
            self.screen.blit(phase_text, (PANEL_X + 22, y_offset + 7))
            y_offset += 45
        else:
            y_offset += 10

        # Divider
        pygame.draw.line(self.screen, (70, 75, 85),
                        (PANEL_X + 20, y_offset), (PANEL_X + PANEL_WIDTH - 20, y_offset))
        y_offset += 20

        # Players info (compact)
        for color in [BLACK, WHITE]:
            player = state.players[color]
            is_current = state.current_turn == color and not state.is_game_over
            is_winner = state.is_game_over and state.winner == color
            captures = state.captures[color]

            # Highlight box for current/winner
            box_height = 70
            if is_current or is_winner:
                highlight_rect = pygame.Rect(PANEL_X + 12, y_offset - 5, PANEL_WIDTH - 24, box_height)
                if is_winner:
                    pygame.draw.rect(self.screen, (55, 50, 30), highlight_rect, border_radius=8)
                    pygame.draw.rect(self.screen, COLOR_WIN_HIGHLIGHT, highlight_rect, 2, border_radius=8)
                else:
                    pygame.draw.rect(self.screen, (50, 58, 72), highlight_rect, border_radius=8)

            # Stone icon
            stone_x = PANEL_X + 32
            stone_y = y_offset + 15
            stone_color = COLOR_BLACK_STONE if color == BLACK else COLOR_WHITE_STONE
            pygame.draw.circle(self.screen, (30, 30, 30), (stone_x + 2, stone_y + 2), 12)
            pygame.draw.circle(self.screen, stone_color, (stone_x, stone_y), 12)
            if color == WHITE:
                pygame.draw.circle(self.screen, (255, 255, 255), (stone_x - 3, stone_y - 3), 2)

            # Player name
            name_color = COLOR_WIN_HIGHLIGHT if is_winner else COLOR_TEXT
            name = self.font_medium.render(player.name, True, name_color)
            self.screen.blit(name, (PANEL_X + 52, y_offset + 3))

            # Capture bar (compact)
            bar_x = PANEL_X + 52
            bar_y = y_offset + 32
            bar_width = PANEL_WIDTH - 115
            bar_height = 12

            # Background
            pygame.draw.rect(self.screen, COLOR_CAPTURE_BG,
                           (bar_x, bar_y, bar_width, bar_height), border_radius=6)
            # Progress
            progress_width = int(bar_width * min(captures, 10) / 10)
            if progress_width > 0:
                bar_color = COLOR_WIN_HIGHLIGHT if captures >= 10 else COLOR_CAPTURE_BAR
                pygame.draw.rect(self.screen, bar_color,
                               (bar_x, bar_y, progress_width, bar_height), border_radius=6)

            # Capture count and AI time on same line
            cap_text = f"Cap: {captures}/10"
            cap_label = self.font_small.render(cap_text, True, (150, 150, 150))
            self.screen.blit(cap_label, (bar_x, bar_y + 16))

            # Show AI move time for AI players
            if player.player_type.value == "ai":
                move_time = state.last_move_time.get(color, 0.0)
                if move_time > 0:
                    # Color based on time (green < 0.3s, yellow < 0.5s, red >= 0.5s)
                    if move_time >= 0.5:
                        time_color = (255, 100, 100)  # Red - too slow
                    elif move_time >= 0.3:
                        time_color = (255, 200, 100)  # Yellow - warning
                    else:
                        time_color = (100, 220, 150)  # Green - good
                    time_text = f"{move_time:.2f}s"
                    time_label = self.font_small.render(time_text, True, time_color)
                    self.screen.blit(time_label, (bar_x + 90, bar_y + 16))

            y_offset += box_height + 8

        # Divider
        y_offset += 5
        pygame.draw.line(self.screen, (70, 75, 85),
                        (PANEL_X + 20, y_offset), (PANEL_X + PANEL_WIDTH - 20, y_offset))
        y_offset += 20

        # Game status (AI timer during play)
        if not state.is_game_over:
            # Show AI timer only during AI turn
            if state.is_ai_turn():
                elapsed = state.get_ai_elapsed_time()
                if elapsed > 0.4:
                    timer_color = (255, 100, 100)
                elif elapsed > 0.3:
                    timer_color = (255, 200, 100)
                else:
                    timer_color = (100, 220, 150)

                timer_text = f"AI: {elapsed:.2f}s"
                timer = self.font_medium.render(timer_text, True, timer_color)
                self.screen.blit(timer, (PANEL_X + 20, y_offset))
                y_offset += 35
            else:
                y_offset += 10
        else:
            # Game over - no timer needed
            y_offset += 10

        # Buttons
        y_offset += 10
        self._render_buttons(state, y_offset)
        y_offset += 100  # After buttons

        # Game over result (in panel, below buttons)
        if state.is_game_over:
            # Result box
            result_rect = pygame.Rect(PANEL_X + 15, y_offset, PANEL_WIDTH - 30, 70)
            pygame.draw.rect(self.screen, (55, 50, 35), result_rect, border_radius=8)
            pygame.draw.rect(self.screen, COLOR_WIN_HIGHLIGHT, result_rect, 2, border_radius=8)

            # Winner text
            if state.winner == BLACK:
                winner_text = "BLACK WINS!"
            elif state.winner == WHITE:
                winner_text = "WHITE WINS!"
            else:
                winner_text = "DRAW!"
            winner = self.font_large.render(winner_text, True, COLOR_WIN_HIGHLIGHT)
            winner_x = PANEL_X + (PANEL_WIDTH - winner.get_width()) // 2
            self.screen.blit(winner, (winner_x, y_offset + 8))

            # Win reason
            if state.winner != EMPTY:
                reason = "by Capture" if state.captures.get(state.winner, 0) >= 10 else "by 5 in a row"
                reason_text = self.font_small.render(reason, True, (180, 180, 180))
                reason_x = PANEL_X + (PANEL_WIDTH - reason_text.get_width()) // 2
                self.screen.blit(reason_text, (reason_x, y_offset + 45))
            y_offset += 80

        # Help hint at bottom of panel
        hint_y = BOARD_MARGIN + BOARD_AREA_SIZE - 25
        hint = self.font_small.render("Press ? for help  •  I for rules", True, (100, 105, 115))
        hint_x = PANEL_X + (PANEL_WIDTH - hint.get_width()) // 2
        self.screen.blit(hint, (hint_x, hint_y))

    def _render_buttons(self, state: GameState = None, start_y: int = 480):
        """Render buttons with improved styling."""
        mouse_pos = pygame.mouse.get_pos()

        button_x = PANEL_X + 18
        button_y = start_y

        # Update button positions dynamically
        self.buttons = {
            'new_game': pygame.Rect(button_x, button_y, self.button_width, self.button_height),
            'undo': pygame.Rect(button_x + 128, button_y, self.button_width, self.button_height),
            'suggest': pygame.Rect(button_x, button_y + 46, self.button_width, self.button_height),
            'mode': pygame.Rect(button_x + 128, button_y + 46, self.button_width, self.button_height),
        }

        button_labels = {
            'new_game': 'New Game',
            'undo': 'Undo',
            'suggest': 'Suggest',
            'mode': 'Mode',
        }

        for name, rect in self.buttons.items():
            is_hover = rect.collidepoint(mouse_pos)

            if is_hover:
                color = (75, 85, 100)
                border_color = COLOR_HIGHLIGHT
            else:
                color = (55, 62, 75)
                border_color = (80, 85, 95)

            # Shadow
            shadow_rect = pygame.Rect(rect.x + 2, rect.y + 2, rect.width, rect.height)
            pygame.draw.rect(self.screen, (25, 28, 35), shadow_rect, border_radius=8)

            # Button
            pygame.draw.rect(self.screen, color, rect, border_radius=8)
            pygame.draw.rect(self.screen, border_color, rect, 1, border_radius=8)

            # Label
            label = self.font_small.render(button_labels[name], True, COLOR_TEXT)
            label_x = rect.centerx - label.get_width() // 2
            label_y = rect.centery - label.get_height() // 2
            self.screen.blit(label, (label_x, label_y))

    def _render_game_over_overlay(self, state: GameState):
        """Render a prominent game over overlay without hiding the board."""
        # No full-screen overlay - keep board fully visible

        # Result box below board (doesn't block board view)
        box_width = 500
        box_height = 100
        box_x = (BOARD_MARGIN + BOARD_AREA_SIZE // 2) - box_width // 2
        box_y = BOARD_MARGIN + BOARD_AREA_SIZE + 20  # Position below board

        # Animated border
        elapsed = time.time() - self.win_animation_start
        pulse = 0.8 + 0.2 * abs((elapsed * 2) % 2 - 1)

        # Box background
        box_rect = pygame.Rect(box_x, box_y, box_width, box_height)
        pygame.draw.rect(self.screen, (40, 45, 55), box_rect, border_radius=15)

        # Gold border for winner
        border_width = int(4 * pulse)
        pygame.draw.rect(self.screen, COLOR_WIN_HIGHLIGHT, box_rect, border_width, border_radius=15)

        # Winner text
        if state.winner == BLACK:
            winner_text = "BLACK WINS!"
            winner_color = COLOR_BLACK_STONE
        elif state.winner == WHITE:
            winner_text = "WHITE WINS!"
            winner_color = COLOR_WHITE_STONE
        else:
            winner_text = "DRAW!"
            winner_color = COLOR_TEXT

        # Winner announcement (no emoji - pygame font issue)
        winner = self.font_large.render(winner_text, True, COLOR_WIN_HIGHLIGHT)
        winner_x = box_x + box_width // 2 - winner.get_width() // 2
        self.screen.blit(winner, (winner_x, box_y + 15))

        # Win reason and instruction on same line
        if state.winner != EMPTY:
            if state.captures.get(state.winner, 0) >= 10:
                reason = f"by Capture ({state.captures[state.winner]} stones)"
            else:
                reason = "by 5 in a row"
            reason_text = self.font_medium.render(reason, True, (180, 180, 180))
            reason_x = box_x + box_width // 2 - reason_text.get_width() // 2
            self.screen.blit(reason_text, (reason_x, box_y + 55))

        # Instructions
        instruction = self.font_small.render("Press N for New Game", True, (150, 150, 150))
        instr_x = box_x + box_width // 2 - instruction.get_width() // 2
        self.screen.blit(instruction, (instr_x, box_y + 80))

    def _render_debug_panel(self, debug_info: dict):
        """Render the debug panel with comprehensive AI performance info."""
        # Semi-transparent background
        panel_width = 350
        panel_height = 580
        panel_x = BOARD_MARGIN + 10
        panel_y = BOARD_MARGIN + 10

        s = pygame.Surface((panel_width, panel_height), pygame.SRCALPHA)
        s.fill((30, 30, 40, 230))
        self.screen.blit(s, (panel_x, panel_y))

        # Border
        pygame.draw.rect(self.screen, COLOR_TEXT,
                        (panel_x, panel_y, panel_width, panel_height), 1)

        y = panel_y + 12

        # Title
        title = self.font_medium.render("AI Performance", True, COLOR_HIGHLIGHT)
        self.screen.blit(title, (panel_x + 15, y))
        y += 30

        # === Search Stats ===
        section = self.font_small.render("[ Search ]", True, (150, 180, 255))
        self.screen.blit(section, (panel_x + 15, y))
        y += 20

        nodes = debug_info.get('nodes_evaluated', 0)
        nps = debug_info.get('nodes_per_second', 0)
        search_lines = [
            f"Time: {debug_info.get('thinking_time', 0):.3f}s",
            f"Depth: {debug_info.get('search_depth', 0)}",
            f"Nodes: {nodes:,}",
            f"NPS: {nps:,.0f}",
        ]
        for line in search_lines:
            text = self.font_small.render(line, True, COLOR_TEXT)
            self.screen.blit(text, (panel_x + 25, y))
            y += 18

        y += 8

        # === Pruning Stats ===
        section = self.font_small.render("[ Pruning ]", True, (150, 180, 255))
        self.screen.blit(section, (panel_x + 15, y))
        y += 20

        alpha_cuts = debug_info.get('alpha_cutoffs', 0)
        beta_cuts = debug_info.get('beta_cutoffs', 0)
        null_cuts = debug_info.get('null_cutoffs', 0)
        lmr_red = debug_info.get('lmr_reductions', 0)
        lmr_res = debug_info.get('lmr_researches', 0)

        # Calculate efficiency
        total_cuts = alpha_cuts + beta_cuts + null_cuts
        cut_rate = (total_cuts / nodes * 100) if nodes > 0 else 0
        lmr_success = ((lmr_red - lmr_res) / lmr_red * 100) if lmr_red > 0 else 0

        prune_lines = [
            f"Alpha: {alpha_cuts:,}  Beta: {beta_cuts:,}",
            f"Null Move: {null_cuts:,}",
            f"LMR: {lmr_red:,} ({lmr_success:.0f}% saved)",
            f"Cut Rate: {cut_rate:.1f}%",
        ]
        for line in prune_lines:
            text = self.font_small.render(line, True, COLOR_TEXT)
            self.screen.blit(text, (panel_x + 25, y))
            y += 18

        y += 8

        # === Transposition Table ===
        section = self.font_small.render("[ TT Cache ]", True, (150, 180, 255))
        self.screen.blit(section, (panel_x + 15, y))
        y += 20

        tt_hit = debug_info.get('tt_hit_rate', '0%')
        tt_fill = debug_info.get('tt_filled', '0%')

        tt_lines = [
            f"Hit Rate: {tt_hit}",
            f"Fill: {tt_fill}",
        ]
        for line in tt_lines:
            text = self.font_small.render(line, True, COLOR_TEXT)
            self.screen.blit(text, (panel_x + 25, y))
            y += 18

        y += 8

        # === Best Move ===
        section = self.font_small.render("[ Result ]", True, (150, 180, 255))
        self.screen.blit(section, (panel_x + 15, y))
        y += 20

        best_move = debug_info.get('best_move', None)
        best_score = debug_info.get('best_score', 0)
        if best_move:
            move_str = f"Move: ({best_move[0]},{best_move[1]})"
        else:
            move_str = "Move: N/A"
        score_str = f"Score: {best_score:+,}"

        text = self.font_small.render(move_str, True, COLOR_TEXT)
        self.screen.blit(text, (panel_x + 25, y))
        y += 18
        text = self.font_small.render(score_str, True, COLOR_TEXT)
        self.screen.blit(text, (panel_x + 25, y))
        y += 22

        # PV Line
        pv_line = debug_info.get('pv_line', [])
        if pv_line:
            pv_label = self.font_small.render("PV:", True, (120, 120, 120))
            self.screen.blit(pv_label, (panel_x + 25, y))
            pv_str = " ".join([f"({m[0]},{m[1]})" for m in pv_line[:4]])
            pv_text = self.font_small.render(pv_str, True, (150, 200, 255))
            self.screen.blit(pv_text, (panel_x + 55, y))
        y += 22

        # === Top Candidates ===
        section = self.font_small.render("[ Top Moves ]", True, (150, 180, 255))
        self.screen.blit(section, (panel_x + 15, y))
        y += 20

        top_moves = debug_info.get('top_moves', [])
        if top_moves:
            max_score = abs(top_moves[0][1]) if top_moves[0][1] != 0 else 1
            for i, (move, score) in enumerate(top_moves[:5]):
                # Bar
                bar_width = min(130, int(130 * abs(score) / max_score))
                bar_color = (100, 150, 255) if score >= 0 else (255, 100, 100)
                pygame.draw.rect(self.screen, bar_color,
                               (panel_x + 150, y + 2, bar_width, 14))

                # Text
                move_text = f"{i+1}. ({move[0]},{move[1]}) {score:+}"
                text = self.font_small.render(move_text, True, COLOR_TEXT)
                self.screen.blit(text, (panel_x + 25, y))
                y += 18

    def _render_help_overlay(self):
        """Render keyboard shortcuts help overlay."""
        # Semi-transparent background
        overlay = pygame.Surface((WINDOW_WIDTH, WINDOW_HEIGHT), pygame.SRCALPHA)
        overlay.fill((0, 0, 0, 200))
        self.screen.blit(overlay, (0, 0))

        # Central box (increased height for spacing)
        box_width = 420
        box_height = 480
        box_x = (WINDOW_WIDTH - box_width) // 2
        box_y = (WINDOW_HEIGHT - box_height) // 2

        pygame.draw.rect(self.screen, (45, 50, 60),
                        (box_x, box_y, box_width, box_height), border_radius=12)
        pygame.draw.rect(self.screen, COLOR_HIGHLIGHT,
                        (box_x, box_y, box_width, box_height), 2, border_radius=12)

        y = box_y + 20

        # Title
        title = self.font_large.render("KEYBOARD SHORTCUTS", True, COLOR_HIGHLIGHT)
        title_x = box_x + (box_width - title.get_width()) // 2
        self.screen.blit(title, (title_x, y))
        y += 50

        # Shortcuts grouped
        sections = [
            ("Game", [
                ("N", "New Game"),
                ("U / Z", "Undo Move"),
                ("S", "Suggest Move"),
                ("M", "Toggle Mode"),
            ]),
            ("Settings", [
                ("R", "Change Rule"),
                ("L", "AI Level"),
                ("D", "Debug Info"),
                ("V", "Show Valid Moves"),
            ]),
            ("Swap Rules", [
                ("B / W", "Choose Black/White"),
                ("1 / 2 / 3", "Swap2 Options"),
            ]),
        ]

        for section_name, shortcuts in sections:
            # Section header
            header = self.font_medium.render(section_name, True, (180, 180, 180))
            self.screen.blit(header, (box_x + 30, y))
            y += 28

            for key, desc in shortcuts:
                key_text = self.font_small.render(key, True, COLOR_HIGHLIGHT)
                desc_text = self.font_small.render(desc, True, COLOR_TEXT)
                self.screen.blit(key_text, (box_x + 50, y))
                self.screen.blit(desc_text, (box_x + 140, y))
                y += 24

            y += 18

        # Close hint (positioned at bottom with adequate spacing)
        close_hint = self.font_small.render("Press ? or ESC to close", True, (120, 120, 120))
        hint_x = box_x + (box_width - close_hint.get_width()) // 2
        self.screen.blit(close_hint, (hint_x, box_y + box_height - 35))

    def _render_rules_overlay(self, state: GameState):
        """Render current rule information overlay."""
        overlay = pygame.Surface((WINDOW_WIDTH, WINDOW_HEIGHT), pygame.SRCALPHA)
        overlay.fill((0, 0, 0, 200))
        self.screen.blit(overlay, (0, 0))

        box_width = 450
        box_height = 420
        box_x = (WINDOW_WIDTH - box_width) // 2
        box_y = (WINDOW_HEIGHT - box_height) // 2

        pygame.draw.rect(self.screen, (45, 50, 60),
                        (box_x, box_y, box_width, box_height), border_radius=12)
        pygame.draw.rect(self.screen, (100, 180, 255),
                        (box_x, box_y, box_width, box_height), 2, border_radius=12)

        y = box_y + 20

        # Rule name
        rule_names = {
            StartingRule.STANDARD: "STANDARD",
            StartingRule.PRO: "PRO RULE",
            StartingRule.SWAP: "SWAP",
            StartingRule.SWAP2: "SWAP2",
        }
        rule_name = rule_names.get(state.starting_rule, "UNKNOWN")
        title = self.font_large.render(f"RULE: {rule_name}", True, (100, 180, 255))
        title_x = box_x + (box_width - title.get_width()) // 2
        self.screen.blit(title, (title_x, y))
        y += 55

        # Rule description
        rule_desc = {
            StartingRule.STANDARD: [
                "No restrictions on opening moves.",
                "Players alternate turns freely.",
            ],
            StartingRule.PRO: [
                "1st move: Black must play at center",
                "2nd move: White plays anywhere",
                "3rd move: Black must be 3+ away",
                "         from center (Chebyshev)",
            ],
            StartingRule.SWAP: [
                "Player 1 places 3 stones:",
                "  Black, White, Black",
                "",
                "Player 2 chooses color (B/W keys)",
            ],
            StartingRule.SWAP2: [
                "Player 1 places 3 stones",
                "",
                "Player 2 chooses (1/2/3 keys):",
                "  1: Play as Black",
                "  2: Play as White",
                "  3: Place 2 more, P1 chooses",
            ],
        }

        header = self.font_medium.render("Opening Rule:", True, (180, 180, 180))
        self.screen.blit(header, (box_x + 30, y))
        y += 30

        for line in rule_desc.get(state.starting_rule, []):
            text = self.font_small.render(line, True, COLOR_TEXT)
            self.screen.blit(text, (box_x + 40, y))
            y += 24

        y += 20

        # Win conditions
        header2 = self.font_medium.render("Win Conditions:", True, (180, 180, 180))
        self.screen.blit(header2, (box_x + 30, y))
        y += 30

        win_rules = [
            "• 5 or more stones in a row",
            "• Capture 10 opponent stones",
        ]
        for line in win_rules:
            text = self.font_small.render(line, True, COLOR_TEXT)
            self.screen.blit(text, (box_x + 40, y))
            y += 24

        y += 20

        # Special rules
        header3 = self.font_medium.render("Special Rules:", True, (180, 180, 180))
        self.screen.blit(header3, (box_x + 30, y))
        y += 30

        special_rules = [
            "* Capture: X-O-O-X removes O-O",
            "* Double-three is forbidden",
            "* 5-row can be broken by capture",
        ]
        for line in special_rules:
            text = self.font_small.render(line, True, COLOR_TEXT)
            self.screen.blit(text, (box_x + 40, y))
            y += 24

        # Close hint
        close_hint = self.font_small.render("Press I or ESC to close", True, (120, 120, 120))
        hint_x = box_x + (box_width - close_hint.get_width()) // 2
        self.screen.blit(close_hint, (hint_x, box_y + box_height - 35))

    def toggle_help_overlay(self):
        """Toggle help overlay visibility."""
        self.show_help_overlay = not self.show_help_overlay
        if self.show_help_overlay:
            self.show_rules_overlay = False

    def toggle_rules_overlay(self):
        """Toggle rules overlay visibility."""
        self.show_rules_overlay = not self.show_rules_overlay
        if self.show_rules_overlay:
            self.show_help_overlay = False

    def close_overlays(self):
        """Close all overlays."""
        self.show_help_overlay = False
        self.show_rules_overlay = False

    def get_button_at(self, pos: tuple) -> Optional[str]:
        """Get the button name at a screen position."""
        for name, rect in self.buttons.items():
            if rect.collidepoint(pos):
                return name
        return None

    def update_hover(self, pos: tuple):
        """Update hover position for move preview."""
        self.hover_pos = self.screen_to_board(pos[0], pos[1])

    def tick(self, fps: int = 60):
        """Control frame rate."""
        self.clock.tick(fps)

    def quit(self):
        """Clean up pygame."""
        pygame.quit()
