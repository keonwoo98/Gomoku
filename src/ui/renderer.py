"""
Pygame renderer for Gomoku.
Handles all visual rendering of the game.
"""

import pygame
from typing import Optional

from ..game.board import BOARD_SIZE, BLACK, WHITE, EMPTY
from ..game.state import GameState, GameMode

# Window settings
WINDOW_WIDTH = 1000
WINDOW_HEIGHT = 720

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
        self.font_medium = pygame.font.Font(None, 32)
        self.font_small = pygame.font.Font(None, 24)

        # Hover state
        self.hover_pos: Optional[tuple] = None
        self.show_valid_moves = False

        # Buttons
        self.buttons = {}
        self._setup_buttons()

    def _setup_buttons(self):
        """Setup button positions and sizes."""
        button_width = 120
        button_height = 35
        button_x = PANEL_X + 20
        button_y = 480

        self.buttons = {
            'new_game': pygame.Rect(button_x, button_y, button_width, button_height),
            'undo': pygame.Rect(button_x + 130, button_y, button_width, button_height),
            'suggest': pygame.Rect(button_x, button_y + 45, button_width, button_height),
            'mode': pygame.Rect(button_x + 130, button_y + 45, button_width, button_height),
        }

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

    def render(self, state: GameState, suggested_move: Optional[tuple] = None,
               debug_info: Optional[dict] = None, show_debug: bool = False):
        """Render the complete game state."""
        # Background
        self.screen.fill(COLOR_BG)

        # Board
        self._render_board(state, suggested_move)

        # Side panel
        self._render_panel(state)

        # Debug panel (if enabled)
        if show_debug and debug_info:
            self._render_debug_panel(debug_info)

        pygame.display.flip()

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

    def _render_panel(self, state: GameState):
        """Render the side panel with game info."""
        # Panel background
        panel_rect = pygame.Rect(PANEL_X, BOARD_MARGIN, PANEL_WIDTH, BOARD_AREA_SIZE)
        pygame.draw.rect(self.screen, COLOR_PANEL_BG, panel_rect, border_radius=10)

        y_offset = BOARD_MARGIN + 20

        # Title
        title = self.font_large.render("GOMOKU", True, COLOR_TEXT)
        self.screen.blit(title, (PANEL_X + 20, y_offset))
        y_offset += 60

        # Game mode
        mode_text = {
            GameMode.PVP: "Player vs Player",
            GameMode.PVE: "Player vs AI",
            GameMode.EVE: "AI vs AI",
        }.get(state.mode, "Unknown")
        mode = self.font_small.render(mode_text, True, COLOR_TEXT)
        self.screen.blit(mode, (PANEL_X + 20, y_offset))
        y_offset += 40

        # Divider
        pygame.draw.line(self.screen, COLOR_TEXT,
                        (PANEL_X + 20, y_offset), (PANEL_X + PANEL_WIDTH - 20, y_offset))
        y_offset += 20

        # Players info
        for color in [BLACK, WHITE]:
            player = state.players[color]
            is_current = state.current_turn == color and not state.is_game_over

            # Highlight current player
            if is_current:
                highlight_rect = pygame.Rect(PANEL_X + 10, y_offset - 5, PANEL_WIDTH - 20, 60)
                pygame.draw.rect(self.screen, (60, 70, 80), highlight_rect, border_radius=5)

            # Stone icon
            stone_x = PANEL_X + 35
            stone_y = y_offset + 20
            stone_color = COLOR_BLACK_STONE if color == BLACK else COLOR_WHITE_STONE
            pygame.draw.circle(self.screen, stone_color, (stone_x, stone_y), 12)

            # Player name
            name = self.font_medium.render(player.name, True, COLOR_TEXT)
            self.screen.blit(name, (PANEL_X + 60, y_offset + 5))

            # Captures
            captures_text = f"Captures: {state.captures[color]}"
            captures = self.font_small.render(captures_text, True, COLOR_TEXT)
            self.screen.blit(captures, (PANEL_X + 60, y_offset + 35))

            y_offset += 70

        y_offset += 10

        # Divider
        pygame.draw.line(self.screen, COLOR_TEXT,
                        (PANEL_X + 20, y_offset), (PANEL_X + PANEL_WIDTH - 20, y_offset))
        y_offset += 20

        # Current turn or winner
        if state.is_game_over:
            if state.winner == BLACK:
                status_text = "Black Wins!"
            elif state.winner == WHITE:
                status_text = "White Wins!"
            else:
                status_text = "Draw!"
            status_color = COLOR_HIGHLIGHT
        else:
            turn_name = "Black" if state.current_turn == BLACK else "White"
            status_text = f"{turn_name}'s Turn"
            status_color = COLOR_TEXT

        status = self.font_medium.render(status_text, True, status_color)
        self.screen.blit(status, (PANEL_X + 20, y_offset))
        y_offset += 40

        # Move count
        move_text = f"Move: #{state.get_move_count() + 1}"
        move = self.font_small.render(move_text, True, COLOR_TEXT)
        self.screen.blit(move, (PANEL_X + 20, y_offset))
        y_offset += 40

        # AI Timer
        y_offset += 10
        timer_label = self.font_medium.render("AI Timer", True, COLOR_TEXT)
        self.screen.blit(timer_label, (PANEL_X + 20, y_offset))
        y_offset += 30

        elapsed = state.get_ai_elapsed_time()
        timer_color = (255, 100, 100) if elapsed > 0.4 else COLOR_TEXT
        timer_text = f"{elapsed:.3f}s"
        timer = self.font_large.render(timer_text, True, timer_color)
        self.screen.blit(timer, (PANEL_X + 20, y_offset))
        y_offset += 60

        # Buttons
        self._render_buttons()

        # Instructions
        y_offset = BOARD_MARGIN + BOARD_AREA_SIZE - 80
        instructions = [
            "D: Toggle debug panel",
            "V: Show valid moves",
            "ESC: Quit",
        ]
        for instruction in instructions:
            text = self.font_small.render(instruction, True, (150, 150, 150))
            self.screen.blit(text, (PANEL_X + 20, y_offset))
            y_offset += 22

    def _render_buttons(self):
        """Render buttons."""
        mouse_pos = pygame.mouse.get_pos()

        button_labels = {
            'new_game': 'New Game',
            'undo': 'Undo',
            'suggest': 'Suggest',
            'mode': 'Mode',
        }

        for name, rect in self.buttons.items():
            # Check hover
            is_hover = rect.collidepoint(mouse_pos)
            color = COLOR_BUTTON_HOVER if is_hover else COLOR_BUTTON

            # Draw button
            pygame.draw.rect(self.screen, color, rect, border_radius=5)
            pygame.draw.rect(self.screen, COLOR_TEXT, rect, 1, border_radius=5)

            # Draw label
            label = self.font_small.render(button_labels[name], True, COLOR_TEXT)
            label_x = rect.centerx - label.get_width() // 2
            label_y = rect.centery - label.get_height() // 2
            self.screen.blit(label, (label_x, label_y))

    def _render_debug_panel(self, debug_info: dict):
        """Render the debug panel with AI information."""
        # Semi-transparent background
        panel_width = 350
        panel_height = 400
        panel_x = BOARD_MARGIN + 10
        panel_y = BOARD_MARGIN + 10

        s = pygame.Surface((panel_width, panel_height), pygame.SRCALPHA)
        s.fill((30, 30, 40, 230))
        self.screen.blit(s, (panel_x, panel_y))

        # Border
        pygame.draw.rect(self.screen, COLOR_TEXT,
                        (panel_x, panel_y, panel_width, panel_height), 1)

        y = panel_y + 15

        # Title
        title = self.font_medium.render("AI Debug Panel", True, COLOR_HIGHLIGHT)
        self.screen.blit(title, (panel_x + 15, y))
        y += 35

        # Debug info lines
        info_lines = [
            f"Thinking Time: {debug_info.get('thinking_time', 0):.3f}s",
            f"Search Depth: {debug_info.get('search_depth', 0)}",
            f"Nodes Evaluated: {debug_info.get('nodes_evaluated', 0):,}",
            f"Nodes/Second: {debug_info.get('nodes_per_second', 0):,.0f}",
            "",
            f"Best Move: {debug_info.get('best_move', 'N/A')}",
            f"Score: {debug_info.get('best_score', 0):+,}",
        ]

        for line in info_lines:
            if line:
                text = self.font_small.render(line, True, COLOR_TEXT)
                self.screen.blit(text, (panel_x + 15, y))
            y += 22

        # Principal Variation
        y += 10
        pv_label = self.font_small.render("Principal Variation:", True, COLOR_TEXT)
        self.screen.blit(pv_label, (panel_x + 15, y))
        y += 22

        pv_line = debug_info.get('pv_line', [])
        if pv_line:
            pv_str = " -> ".join([f"({m[0]},{m[1]})" for m in pv_line[:6]])
            pv_text = self.font_small.render(pv_str, True, (150, 200, 255))
            self.screen.blit(pv_text, (panel_x + 15, y))
        y += 30

        # Top candidates
        top_label = self.font_small.render("Top Candidates:", True, COLOR_TEXT)
        self.screen.blit(top_label, (panel_x + 15, y))
        y += 22

        top_moves = debug_info.get('top_moves', [])
        if top_moves:
            max_score = abs(top_moves[0][1]) if top_moves[0][1] != 0 else 1
            for i, (move, score) in enumerate(top_moves[:5]):
                # Bar
                bar_width = min(150, int(150 * abs(score) / max_score))
                bar_color = (100, 150, 255) if score >= 0 else (255, 100, 100)
                pygame.draw.rect(self.screen, bar_color,
                               (panel_x + 150, y + 2, bar_width, 14))

                # Text
                move_text = f"{i+1}. ({move[0]},{move[1]}) {score:+}"
                text = self.font_small.render(move_text, True, COLOR_TEXT)
                self.screen.blit(text, (panel_x + 15, y))
                y += 20

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
