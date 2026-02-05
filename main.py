#!/usr/bin/env python3
"""
Gomoku - AI vs Human Board Game
Main entry point for the game.
"""

import sys
import pygame

from src.game.state import GameState, GameMode, StartingRule, GamePhase, AIDifficulty
from src.game.board import BLACK, WHITE
from src.game.rules import Rules
from src.ai.engine import AIEngine
from src.ui.renderer import Renderer
from src.ui.input import InputHandler, InputAction


class GomokuGame:
    """Main game controller."""

    def __init__(self):
        self.renderer = Renderer()
        self.input_handler = InputHandler(self.renderer)
        self.ai_engine = AIEngine()

        # Game state
        self.state = GameState(GameMode.PVE)

        # UI state
        self.show_debug = False
        self.suggested_move = None
        self.running = True

        # Mode cycle
        self.modes = [GameMode.PVE, GameMode.PVP, GameMode.EVE]
        self.current_mode_idx = 0

        # Starting rule cycle
        self.rules = [
            StartingRule.STANDARD,
            StartingRule.PRO,
            StartingRule.SWAP,
            StartingRule.SWAP2
        ]
        self.current_rule_idx = 0

        # AI difficulty cycle
        self.difficulties = [
            AIDifficulty.EASY,
            AIDifficulty.MEDIUM,
            AIDifficulty.HARD,
            AIDifficulty.EXPERT
        ]
        self.current_difficulty_idx = 3  # Default: EXPERT
        self._apply_difficulty()

    def run(self):
        """Main game loop."""
        while self.running:
            # Process input
            self._handle_input()

            # AI turn (only in normal phase, not during special phases)
            if (self.state.is_ai_turn() and
                not self.state.is_game_over and
                self.state.phase == GamePhase.NORMAL):
                self._run_ai_turn()

            # Render
            debug_info = self.ai_engine.get_debug_info() if self.show_debug else None
            self.renderer.render(
                self.state,
                suggested_move=self.suggested_move,
                debug_info=debug_info,
                show_debug=self.show_debug,
                difficulty=self.get_current_difficulty()
            )

            # Frame rate control
            self.renderer.tick(60)

        self.renderer.quit()

    def _handle_input(self):
        """Process all input events."""
        events = self.input_handler.process_events()

        for event in events:
            if event.action == InputAction.QUIT:
                # Close overlays first, then quit
                if self.renderer.show_help_overlay or self.renderer.show_rules_overlay:
                    self.renderer.close_overlays()
                else:
                    self.running = False

            elif event.action == InputAction.PLACE_STONE:
                # Allow placing stones during opening phase or normal human turn
                if event.position:
                    can_place = (
                        self.state.phase in [GamePhase.OPENING_PLACE, GamePhase.SWAP2_EXTRA]
                        or self.state.is_human_turn()
                    )
                    if can_place:
                        row, col = event.position
                        self._make_move(row, col)

            elif event.action == InputAction.NEW_GAME:
                self._new_game()

            elif event.action == InputAction.UNDO:
                self._undo()

            elif event.action == InputAction.SUGGEST:
                self._suggest_move()

            elif event.action == InputAction.TOGGLE_MODE:
                self._toggle_mode()

            elif event.action == InputAction.TOGGLE_DEBUG:
                self.show_debug = not self.show_debug

            elif event.action == InputAction.TOGGLE_VALID_MOVES:
                self.renderer.show_valid_moves = not self.renderer.show_valid_moves

            elif event.action == InputAction.TOGGLE_RULE:
                self._toggle_rule()

            # Swap color choices
            elif event.action == InputAction.CHOOSE_BLACK:
                self._handle_color_choice(BLACK)

            elif event.action == InputAction.CHOOSE_WHITE:
                self._handle_color_choice(WHITE)

            # Swap2 options
            elif event.action == InputAction.SWAP2_OPTION_1:
                self._handle_swap2_option(1)

            elif event.action == InputAction.SWAP2_OPTION_2:
                self._handle_swap2_option(2)

            elif event.action == InputAction.SWAP2_OPTION_3:
                self._handle_swap2_option(3)

            elif event.action == InputAction.TOGGLE_DIFFICULTY:
                self._toggle_difficulty()

            elif event.action == InputAction.TOGGLE_HELP:
                self.renderer.toggle_help_overlay()

            elif event.action == InputAction.TOGGLE_RULES:
                self.renderer.toggle_rules_overlay()

    def _make_move(self, row: int, col: int, thinking_time: float = 0.0):
        """Make a move and handle UI animations."""
        color = self.state.current_turn

        # Check if move is valid and get reason if not
        reason = Rules.get_invalid_reason(self.state.board, row, col, color)
        if reason:
            self.renderer.show_error(reason)
            return

        # Get capture positions BEFORE making the move
        captured_positions = Rules.get_captured_positions(
            self.state.board, row, col, color
        )

        # Make the move
        if self.state.make_move(row, col, thinking_time):
            self.suggested_move = None

            # Trigger capture animation if captures occurred
            if captured_positions:
                self.renderer.trigger_capture_flash(captured_positions, color)

            # Check for win and set win line
            if self.state.is_game_over and self.state.winner != 0:
                self._set_win_animation()

    def _set_win_animation(self):
        """Set the winning line animation."""
        winner = self.state.winner

        # Check if win by 5-in-row (not capture)
        if self.state.captures.get(winner, 0) < 10:
            # Find the winning line
            if self.state.last_move:
                row, col = self.state.last_move
                win_positions = Rules.get_five_positions(
                    self.state.board, row, col, winner
                )
                if win_positions:
                    self.renderer.set_win_line(win_positions)
                else:
                    # Search the entire board for five-in-row
                    for r in range(19):
                        for c in range(19):
                            if self.state.board.get(r, c) == winner:
                                positions = Rules.get_five_positions(
                                    self.state.board, r, c, winner
                                )
                                if positions:
                                    self.renderer.set_win_line(positions)
                                    return
        else:
            # Win by capture - start animation without line
            self.renderer.win_animation_start = __import__('time').time()

    def _run_ai_turn(self):
        """Execute AI move."""
        self.state.start_ai_timer()

        # Get AI move
        move = self.ai_engine.get_move(
            self.state.board,
            self.state.current_turn,
            self.state.captures,
            time_limit=0.5
        )

        self.state.stop_ai_timer()

        if move:
            self._make_move(move[0], move[1], self.state.last_ai_time)

    def _new_game(self):
        """Start a new game."""
        self.state.reset()
        self.suggested_move = None
        self.ai_engine.move_gen.clear()
        self.renderer.reset_animations()

    def _undo(self):
        """Undo the last move(s)."""
        # Undo both player and AI moves in PVE mode
        if self.state.mode == GameMode.PVE:
            # Need at least 2 moves to undo both AI and player
            if self.state.get_move_count() >= 2:
                self.state.undo_move()  # Undo AI move
                self.state.undo_move()  # Undo player move
            elif self.state.get_move_count() == 1:
                self.state.undo_move()  # Only undo player's first move
        else:
            self.state.undo_move()
        self.suggested_move = None
        self.renderer.reset_animations()

    def _suggest_move(self):
        """Get AI suggestion for current player."""
        if self.state.is_game_over:
            return

        # Only suggest for human players
        if not self.state.is_human_turn():
            return

        self.state.start_ai_timer()
        self.suggested_move = self.ai_engine.suggest_move(
            self.state.board,
            self.state.current_turn,
            self.state.captures,
            time_limit=0.3
        )
        self.state.stop_ai_timer()

    def _toggle_mode(self):
        """Toggle between game modes."""
        self.current_mode_idx = (self.current_mode_idx + 1) % len(self.modes)
        new_mode = self.modes[self.current_mode_idx]
        self.state.reset(new_mode)
        self.suggested_move = None
        self.renderer.reset_animations()

    def _toggle_rule(self):
        """Toggle between starting rules."""
        self.current_rule_idx = (self.current_rule_idx + 1) % len(self.rules)
        new_rule = self.rules[self.current_rule_idx]
        self.state.reset(starting_rule=new_rule)
        self.suggested_move = None
        self.ai_engine.move_gen.clear()
        self.renderer.reset_animations()

    def _handle_color_choice(self, color: int):
        """Handle color choice in Swap phases."""
        if self.state.phase == GamePhase.SWAP_CHOICE:
            self.state.choose_color(color)
        elif self.state.phase == GamePhase.SWAP2_FINAL:
            self.state.choose_color(color)

    def _handle_swap2_option(self, option: int):
        """Handle Swap2 option choice."""
        if self.state.phase == GamePhase.SWAP2_CHOICE:
            self.state.choose_swap2_option(option)

    def _toggle_difficulty(self):
        """Toggle AI difficulty level."""
        self.current_difficulty_idx = (self.current_difficulty_idx + 1) % len(self.difficulties)
        self._apply_difficulty()

    def _apply_difficulty(self):
        """Apply current difficulty settings to AI engine."""
        diff = self.difficulties[self.current_difficulty_idx]
        self.ai_engine.set_difficulty(diff.depth, diff.time_limit)

    def get_current_difficulty(self) -> AIDifficulty:
        """Get current AI difficulty."""
        return self.difficulties[self.current_difficulty_idx]


def main():
    """Entry point."""
    try:
        game = GomokuGame()
        game.run()
    except KeyboardInterrupt:
        print("\nGame interrupted.")
        sys.exit(0)
    except Exception as e:
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
