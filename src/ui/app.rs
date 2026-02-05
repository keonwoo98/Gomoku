//! Main application for the Gomoku GUI

use eframe::egui;
use egui::{CentralPanel, Context, RichText, SidePanel, TopBottomPanel};

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
            .min_width(220.0)
            .max_width(280.0)
            .show(ctx, |ui| {
                ui.style_mut().visuals.widgets.noninteractive.bg_fill = PANEL_BG;

                // Game title
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("GOMOKU").size(20.0).strong().color(TEXT_PRIMARY));
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(8.0);

                // Current turn indicator - big and clear
                self.render_turn_indicator(ui);
                ui.add_space(12.0);

                // Timer section
                ui.separator();
                ui.add_space(8.0);
                self.render_timer(ui);
                ui.add_space(8.0);

                // Captures section - visual progress bars
                ui.separator();
                ui.add_space(8.0);
                self.render_captures(ui);
                ui.add_space(8.0);

                // Action buttons
                ui.separator();
                ui.add_space(8.0);
                self.render_action_buttons(ui);
                ui.add_space(8.0);

                // Debug panel (collapsible)
                if self.show_debug {
                    ui.separator();
                    ui.add_space(4.0);
                    self.render_debug_panel(ui);
                }

                // Game over overlay
                if let Some(result) = &self.state.game_over.clone() {
                    ui.separator();
                    ui.add_space(8.0);
                    self.render_game_over(ui, &result);
                }

                // Status message
                if let Some(msg) = &self.state.message {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.colored_label(TIMER_WARNING, msg);
                    });
                }
            });
    }

    /// Render turn indicator
    fn render_turn_indicator(&self, ui: &mut egui::Ui) {
        let (stone_char, color_name) = if self.state.current_turn == Stone::Black {
            ("●", "BLACK")
        } else {
            ("○", "WHITE")
        };

        ui.horizontal(|ui| {
            ui.add_space(12.0);

            // Stone symbol
            let text_color = if self.state.current_turn == Stone::Black {
                TEXT_PRIMARY
            } else {
                STATUS_BLACK
            };
            ui.label(RichText::new(stone_char).size(32.0).color(text_color));

            ui.add_space(8.0);

            ui.vertical(|ui| {
                ui.label(RichText::new(color_name).size(16.0).strong().color(TEXT_PRIMARY));
                let status = if self.state.is_ai_thinking() {
                    "AI thinking..."
                } else if self.state.game_over.is_some() {
                    "Game Over"
                } else {
                    "to move"
                };
                ui.label(RichText::new(status).size(12.0).color(TEXT_SECONDARY));
            });
        });
    }

    /// Render captures with visual progress
    fn render_captures(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("CAPTURES").size(11.0).color(TEXT_MUTED));
        });
        ui.add_space(4.0);

        // Black captures
        self.render_capture_bar(ui, "●", self.state.board.black_captures, STATUS_BLACK, TEXT_PRIMARY);
        ui.add_space(4.0);

        // White captures
        self.render_capture_bar(ui, "○", self.state.board.white_captures, STATUS_WHITE, STATUS_BLACK);
    }

    /// Render a single capture progress bar
    fn render_capture_bar(&self, ui: &mut egui::Ui, symbol: &str, captures: u8, bg: egui::Color32, text_color: egui::Color32) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(symbol).size(16.0).color(text_color));
            ui.add_space(4.0);

            // Progress bar
            let progress = captures as f32 / 5.0;
            let bar_width = 120.0;
            let bar_height = 16.0;

            let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(bar_width, bar_height), egui::Sense::hover());

            // Background
            ui.painter().rect_filled(rect, 4.0, PANEL_HEADER);

            // Filled portion
            if captures > 0 {
                let filled_rect = egui::Rect::from_min_size(
                    rect.min,
                    egui::Vec2::new(bar_width * progress, bar_height),
                );
                let fill_color = if captures >= 4 {
                    TIMER_WARNING // Near win
                } else {
                    bg
                };
                ui.painter().rect_filled(filled_rect, 4.0, fill_color);
            }

            // Border
            ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, PANEL_BORDER), egui::StrokeKind::Outside);

            ui.add_space(8.0);
            ui.label(RichText::new(format!("{}/5", captures)).size(14.0).color(TEXT_PRIMARY));
        });
    }

    /// Render action buttons
    fn render_action_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("ACTIONS").size(11.0).color(TEXT_MUTED));
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add_space(8.0);

            if ui.button(RichText::new("↩ Undo").size(13.0)).clicked() {
                self.state.undo();
            }

            // PvP hint button
            if let GameMode::PvP { .. } = self.state.mode {
                if ui.button(RichText::new("💡 Hint").size(13.0)).clicked() {
                    self.state.request_suggestion();
                }
            }
        });

        // Move count
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Move #{}", self.state.move_history.len())).size(12.0).color(TEXT_SECONDARY));
        });
    }

    /// Render the timer display
    fn render_timer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("TIMER").size(11.0).color(TEXT_MUTED));
        });
        ui.add_space(4.0);

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
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("🤔").size(18.0));
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{:.2}s", secs)).size(24.0).strong().color(color));
                });
            }
        } else {
            let elapsed = self.state.move_timer.elapsed();
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("⏱").size(16.0));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{:.1}s", elapsed.as_secs_f32())).size(18.0).color(TEXT_PRIMARY));
            });
        }

        if let Some(ai_time) = self.state.move_timer.ai_thinking_time {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Last AI: {:.3}s", ai_time.as_secs_f32())).size(11.0).color(TEXT_SECONDARY));
            });
        }
    }

    /// Render the debug panel
    fn render_debug_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("AI DEBUG").size(11.0).color(TEXT_MUTED));
        });
        ui.add_space(4.0);

        if let Some(result) = &self.state.last_ai_result {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("Search: {:?}", result.search_type)).size(11.0).color(TEXT_SECONDARY));
                    ui.label(RichText::new(format!("Score: {}", result.score)).size(11.0).color(TEXT_SECONDARY));
                    ui.label(RichText::new(format!("Nodes: {}", result.nodes)).size(11.0).color(TEXT_SECONDARY));
                    ui.label(RichText::new(format!("Time: {}ms", result.time_ms)).size(11.0).color(TEXT_SECONDARY));

                    if let Some(pos) = result.best_move {
                        let col = (b'A' + pos.col) as char;
                        let row = 19 - pos.row;
                        ui.label(RichText::new(format!("Move: {}{}", col, row)).size(11.0).color(TIMER_NORMAL));
                    }
                });
            });
        } else {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("No AI data yet").size(11.0).color(TEXT_SECONDARY));
            });
        }
    }

    /// Render game over message
    fn render_game_over(&self, ui: &mut egui::Ui, result: &GameResult) {
        let (winner, symbol) = if result.winner == Stone::Black {
            ("BLACK", "●")
        } else {
            ("WHITE", "○")
        };
        let win_type = match result.win_type {
            WinType::FiveInRow => "5-in-a-row",
            WinType::Capture => "10 captures",
        };

        ui.vertical_centered(|ui| {
            ui.label(RichText::new("🎉 GAME OVER").size(16.0).strong().color(WIN_HIGHLIGHT));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("{} {} WINS!", symbol, winner)).size(14.0).strong().color(TEXT_PRIMARY));
            ui.label(RichText::new(format!("by {}", win_type)).size(12.0).color(TEXT_SECONDARY));
            ui.add_space(8.0);
            if ui.button(RichText::new("🔄 New Game").size(14.0)).clicked() {
                // Will be handled in update
            }
        });
    }

    /// Render the main board
    fn render_board(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            // Set board area background
            ui.style_mut().visuals.panel_fill = egui::Color32::from_rgb(40, 42, 46);

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
                self.state.capture_animation.as_ref(),
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

        // Clean up completed capture animations
        if let Some(animation) = &self.state.capture_animation {
            if animation.is_complete() {
                self.state.capture_animation = None;
            }
        }

        // Start AI thinking if needed
        if self.state.is_ai_turn() && !self.state.is_ai_thinking() && self.state.game_over.is_none() {
            self.state.start_ai_thinking();
        }

        // Render UI
        self.render_menu_bar(ctx);
        self.render_side_panel(ctx);
        self.render_board(ctx);

        // Request repaint if animation is playing or AI is thinking
        if self.state.is_ai_thinking() || self.state.capture_animation.is_some() {
            ctx.request_repaint();
        }
    }
}
