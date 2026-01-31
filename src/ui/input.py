"""
Input handling for Gomoku.
Processes mouse and keyboard events.
"""

import pygame
from enum import Enum, auto
from typing import Optional
from dataclasses import dataclass


class InputAction(Enum):
    """Types of input actions."""
    NONE = auto()
    QUIT = auto()
    PLACE_STONE = auto()
    NEW_GAME = auto()
    UNDO = auto()
    SUGGEST = auto()
    TOGGLE_MODE = auto()
    TOGGLE_DEBUG = auto()
    TOGGLE_VALID_MOVES = auto()


@dataclass
class InputEvent:
    """Represents a processed input event."""
    action: InputAction
    position: Optional[tuple] = None  # Board position for PLACE_STONE
    mouse_pos: Optional[tuple] = None  # Screen position


class InputHandler:
    """Handles user input for the game."""

    def __init__(self, renderer):
        self.renderer = renderer
        self.pending_events: list[InputEvent] = []

    def process_events(self) -> list[InputEvent]:
        """
        Process all pending pygame events.
        Returns list of InputEvents.
        """
        events = []

        for event in pygame.event.get():
            input_event = self._process_event(event)
            if input_event and input_event.action != InputAction.NONE:
                events.append(input_event)

        return events

    def _process_event(self, event: pygame.event.Event) -> Optional[InputEvent]:
        """Process a single pygame event."""
        if event.type == pygame.QUIT:
            return InputEvent(InputAction.QUIT)

        elif event.type == pygame.MOUSEMOTION:
            # Update hover position
            self.renderer.update_hover(event.pos)
            return None

        elif event.type == pygame.MOUSEBUTTONDOWN:
            if event.button == 1:  # Left click
                return self._handle_click(event.pos)

        elif event.type == pygame.KEYDOWN:
            return self._handle_keydown(event.key)

        return None

    def _handle_click(self, pos: tuple) -> InputEvent:
        """Handle mouse click."""
        # Check if clicking on a button
        button = self.renderer.get_button_at(pos)
        if button:
            action_map = {
                'new_game': InputAction.NEW_GAME,
                'undo': InputAction.UNDO,
                'suggest': InputAction.SUGGEST,
                'mode': InputAction.TOGGLE_MODE,
            }
            return InputEvent(action_map.get(button, InputAction.NONE), mouse_pos=pos)

        # Check if clicking on board
        board_pos = self.renderer.screen_to_board(pos[0], pos[1])
        if board_pos:
            return InputEvent(InputAction.PLACE_STONE, position=board_pos, mouse_pos=pos)

        return InputEvent(InputAction.NONE)

    def _handle_keydown(self, key: int) -> InputEvent:
        """Handle keyboard input."""
        key_map = {
            pygame.K_ESCAPE: InputAction.QUIT,
            pygame.K_n: InputAction.NEW_GAME,
            pygame.K_u: InputAction.UNDO,
            pygame.K_z: InputAction.UNDO,
            pygame.K_s: InputAction.SUGGEST,
            pygame.K_d: InputAction.TOGGLE_DEBUG,
            pygame.K_v: InputAction.TOGGLE_VALID_MOVES,
            pygame.K_m: InputAction.TOGGLE_MODE,
        }

        action = key_map.get(key, InputAction.NONE)
        return InputEvent(action)

    def wait_for_input(self, timeout_ms: int = 100) -> list[InputEvent]:
        """
        Wait for input with timeout.
        Used during AI thinking to remain responsive.
        """
        # Use pygame.event.wait with timeout for efficiency
        pygame.time.wait(min(timeout_ms, 16))  # Cap at ~60fps
        return self.process_events()
