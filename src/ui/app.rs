//! Main application for the Gomoku GUI

use eframe::egui;
use egui::{CentralPanel, Context, SidePanel, TopBottomPanel};

use crate::Stone;
use super::board_view::BoardView;
use super::game_state::{GameMode, GameResult, GameState, WinType};
use super::theme::*;

/// Main Gomoku application
pub struct GomokuApp {
    state: GameState,
    board_view: BoardView,
    show_debug: bool,
    #[allow(dead_code)]
    show_menu: bool,
}

impl Default for GomokuApp {
    fn default() -> Self {
        Self {
            state: GameState::new(GameMode::default()),
            board_view: BoardView::default(),
            show_debug: true,
            show_menu: false,
        }
    }
}

impl GomokuApp {
    /// Create a new app with the given mode
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    /// Render the top menu bar
    fn render_menu_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Game", |ui| {
                    if ui.button("New Game (PvE - Black)").clicked() {
                        self.state = GameState::new(GameMode::PvE { human_color: Stone::Black });
                        ui.close_menu();
                    }
                    if ui.button("New Game (PvE - White)").clicked() {
                        self.state = GameState::new(GameMode::PvE { human_color: Stone::White });
                        ui.close_menu();
                    }
                    if ui.button("New Game (PvP)").clicked() {
                        self.state = GameState::new(GameMode::PvP { show_suggestions: false });
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Undo").clicked() {
                        self.state.undo();
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_debug, "Debug Panel (D)");
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Show current mode
                    let mode_text = match self.state.mode {
                        GameMode::PvE { human_color } => {
                            format!("PvE - You: {}", if human_color == Stone::Black { "Black" } else { "White" })
                        }
                        GameMode::PvP { .. } => "PvP - Hotseat".to_string(),
                    };
                    ui.label(mode_text);
                });
            });
        });
    }

    /// Render the side panel with game info and debug
    fn render_side_panel(&mut self, ctx: &Context) {
        SidePanel::right("info_panel")
            .min_width(200.0)
            .max_width(300.0)
            .show(ctx, |ui| {
                ui.heading("Game Info");
                ui.separator();

                // Current turn
                let turn_text = if self.state.current_turn == Stone::Black {
                    "⚫ Black's Turn"
                } else {
                    "⚪ White's Turn"
                };
                ui.label(turn_text);

                // Timer
                ui.separator();
                self.render_timer(ui);

                // Captures
                ui.separator();
                ui.label("Captures:");
                ui.horizontal(|ui| {
                    ui.label(format!("⚫ Black: {}/5", self.state.board.black_captures));
                });
                ui.horizontal(|ui| {
                    ui.label(format!("⚪ White: {}/5", self.state.board.white_captures));
                });

                // Move history
                ui.separator();
                ui.label(format!("Moves: {}", self.state.move_history.len()));

                // PvP hint button
                if let GameMode::PvP { .. } = self.state.mode {
                    ui.separator();
                    if ui.button("💡 Get Hint (H)").clicked() {
                        self.state.request_suggestion();
                    }
                }

                // Undo button
                ui.separator();
                if ui.button("↩ Undo (U)").clicked() {
                    self.state.undo();
                }

                // Debug panel
                if self.show_debug {
                    ui.separator();
                    self.render_debug_panel(ui);
                }

                // Game over message
                if let Some(result) = &self.state.game_over {
                    ui.separator();
                    self.render_game_over(ui, result);
                }

                // Error message
                if let Some(msg) = &self.state.message {
                    ui.separator();
                    ui.colored_label(egui::Color32::YELLOW, msg);
                }
            });
    }

    /// Render the timer display
    fn render_timer(&mut self, ui: &mut egui::Ui) {
        ui.label("Timer:");

        if self.state.is_ai_thinking() {
            if let Some(elapsed) = self.state.ai_thinking_elapsed() {
                let secs = elapsed.as_secs_f32();
                let color = if secs < 0.3 {
                    TIMER_NORMAL
                } else if secs < 0.5 {
                    TIMER_WARNING
                } else {
                    TIMER_CRITICAL
                };
                ui.colored_label(color, format!("🤔 AI thinking... {:.2}s", secs));
            }
        } else {
            let elapsed = self.state.move_timer.elapsed();
            ui.label(format!("⏱ Current: {:.1}s", elapsed.as_secs_f32()));
        }

        if let Some(ai_time) = self.state.move_timer.ai_thinking_time {
            ui.label(format!("🤖 Last AI: {:.3}s", ai_time.as_secs_f32()));
        }
    }

    /// Render the debug panel
    fn render_debug_panel(&self, ui: &mut egui::Ui) {
        ui.heading("AI Debug");

        if let Some(result) = &self.state.last_ai_result {
            ui.label(format!("Search: {:?}", result.search_type));
            ui.label(format!("Score: {}", result.score));
            ui.label(format!("Nodes: {}", result.nodes));
            ui.label(format!("Time: {}ms", result.time_ms));

            if let Some(pos) = result.best_move {
                let col = (b'A' + pos.col) as char;
                let row = 19 - pos.row;
                ui.label(format!("Move: {}{}", col, row));
            }
        } else {
            ui.label("No AI data yet");
        }
    }

    /// Render game over message
    fn render_game_over(&self, ui: &mut egui::Ui, result: &GameResult) {
        let winner = if result.winner == Stone::Black { "Black" } else { "White" };
        let win_type = match result.win_type {
            WinType::FiveInRow => "5-in-a-row",
            WinType::Capture => "Capture (10 stones)",
        };

        ui.heading("🎉 Game Over!");
        ui.label(format!("{} wins by {}!", winner, win_type));

        ui.separator();
        if ui.button("🔄 New Game").clicked() {
            // Will be handled in update
        }
    }

    /// Render the main board
    fn render_board(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            let winning_line = self.state.game_over
                .as_ref()
                .and_then(|r| r.winning_line);

            let clicked = self.board_view.show(
                ui,
                &self.state.board,
                self.state.current_turn,
                self.state.last_move,
                self.state.suggested_move,
                winning_line,
                self.state.game_over.is_some(),
            );

            // Handle click
            if let Some(pos) = clicked {
                if let Err(msg) = self.state.try_place_stone(pos) {
                    self.state.message = Some(msg);
                }
            }
        });
    }

    /// Handle keyboard shortcuts
    fn handle_input(&mut self, ctx: &Context) {
        ctx.input(|i| {
            // D - Toggle debug panel
            if i.key_pressed(egui::Key::D) {
                self.show_debug = !self.show_debug;
            }

            // H - Get hint (PvP mode)
            if i.key_pressed(egui::Key::H) {
                if let GameMode::PvP { .. } = self.state.mode {
                    self.state.request_suggestion();
                }
            }

            // U - Undo
            if i.key_pressed(egui::Key::U) {
                self.state.undo();
            }

            // N - New game
            if i.key_pressed(egui::Key::N) {
                self.state.reset();
            }
        });
    }
}

impl eframe::App for GomokuApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        self.handle_input(ctx);

        // Check AI result
        self.state.check_ai_result();

        // Start AI thinking if needed
        if self.state.is_ai_turn() && !self.state.is_ai_thinking() && self.state.game_over.is_none() {
            self.state.start_ai_thinking();
        }

        // Render UI
        self.render_menu_bar(ctx);
        self.render_side_panel(ctx);
        self.render_board(ctx);

        // Request repaint if AI is thinking (for timer update)
        if self.state.is_ai_thinking() {
            ctx.request_repaint();
        }
    }
}
