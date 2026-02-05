//! Main application for the Gomoku GUI

use eframe::egui;
use egui::{CentralPanel, Context, CornerRadius, Frame, RichText, SidePanel, TopBottomPanel, Vec2};

use crate::Stone;
use super::board_view::BoardView;
use super::game_state::{GameMode, GameState, WinType};
use super::theme::*;

/// Main Gomoku application
pub struct GomokuApp {
    state: GameState,
    board_view: BoardView,
    show_debug: bool,
    #[allow(dead_code)]
    show_menu: bool,
    new_game_requested: bool,
}

impl Default for GomokuApp {
    fn default() -> Self {
        Self {
            state: GameState::new(GameMode::default()),
            board_view: BoardView::default(),
            show_debug: true,
            show_menu: false,
            new_game_requested: false,
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
            .max_width(240.0)
            .frame(Frame::new().fill(egui::Color32::from_rgb(30, 32, 36)))
            .show(ctx, |ui| {
                ui.add_space(16.0);

                // Title
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("GOMOKU").size(24.0).strong().color(TEXT_PRIMARY));
                    ui.label(RichText::new("Ninuki-renju").size(10.0).color(TEXT_MUTED));
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                // Turn indicator - using painted circles instead of unicode
                self.render_turn_section(ui);

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                // Timer
                self.render_timer_section(ui);

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                // Captures
                self.render_captures_section(ui);

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                // Actions
                self.render_actions_section(ui);

                // Debug (if enabled)
                if self.show_debug {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    self.render_debug_section(ui);
                }

                // Game over
                if self.state.game_over.is_some() {
                    ui.add_space(12.0);
                    self.render_game_over_section(ui);
                }

                // Message
                if let Some(msg) = &self.state.message {
                    ui.add_space(8.0);
                    ui.colored_label(TIMER_WARNING, msg.as_str());
                }
            });
    }

    /// Render turn indicator with painted stone
    fn render_turn_section(&self, ui: &mut egui::Ui) {
        let is_black = self.state.current_turn == Stone::Black;
        let color_name = if is_black { "BLACK" } else { "WHITE" };

        ui.horizontal(|ui| {
            // Draw actual stone circle
            let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 36.0), egui::Sense::hover());
            let center = rect.center();

            if is_black {
                // Black stone
                ui.painter().circle_filled(center, 16.0, egui::Color32::from_rgb(30, 30, 35));
                ui.painter().circle_stroke(center, 16.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 65)));
            } else {
                // White stone
                ui.painter().circle_filled(center, 16.0, egui::Color32::from_rgb(240, 240, 245));
                ui.painter().circle_stroke(center, 16.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 180, 185)));
            }

            ui.add_space(12.0);

            ui.vertical(|ui| {
                ui.label(RichText::new(color_name).size(16.0).strong().color(TEXT_PRIMARY));

                let (status_text, status_color) = if self.state.is_ai_thinking() {
                    ("AI thinking...", TIMER_WARNING)
                } else if self.state.game_over.is_some() {
                    ("Game Over", WIN_HIGHLIGHT)
                } else {
                    ("to move", TIMER_NORMAL)
                };
                ui.label(RichText::new(status_text).size(11.0).color(status_color));
            });
        });
    }

    /// Render timer section
    fn render_timer_section(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("TIMER").size(10.0).color(TEXT_MUTED));
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
                ui.label(RichText::new(format!("{:.2}s", secs)).size(28.0).strong().color(color));
            }
        } else {
            let elapsed = self.state.move_timer.elapsed();
            ui.label(RichText::new(format!("{:.1}s", elapsed.as_secs_f32())).size(22.0).color(TEXT_PRIMARY));
        }

        if let Some(ai_time) = self.state.move_timer.ai_thinking_time {
            ui.label(RichText::new(format!("Last AI: {:.3}s", ai_time.as_secs_f32())).size(10.0).color(TEXT_SECONDARY));
        }
    }

    /// Render captures section with painted stones
    fn render_captures_section(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("CAPTURES").size(10.0).color(TEXT_MUTED));
        ui.add_space(6.0);

        // Black captures
        self.render_capture_row_painted(ui, true, self.state.board.black_captures);
        ui.add_space(4.0);

        // White captures
        self.render_capture_row_painted(ui, false, self.state.board.white_captures);
    }

    /// Render capture row with painted circles
    fn render_capture_row_painted(&self, ui: &mut egui::Ui, is_black: bool, captures: u8) {
        ui.horizontal(|ui| {
            // Draw 5 stone indicators
            for i in 0..5u8 {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), egui::Sense::hover());
                let center = rect.center();
                let filled = i < captures;
                let near_win = captures >= 4 && filled;

                if is_black {
                    let fill = if near_win {
                        TIMER_WARNING
                    } else if filled {
                        egui::Color32::from_rgb(40, 40, 45)
                    } else {
                        egui::Color32::from_rgb(50, 52, 56)
                    };
                    ui.painter().circle_filled(center, 8.0, fill);
                    if filled {
                        ui.painter().circle_stroke(center, 8.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 70, 75)));
                    }
                } else {
                    let fill = if near_win {
                        TIMER_WARNING
                    } else if filled {
                        egui::Color32::from_rgb(220, 220, 225)
                    } else {
                        egui::Color32::from_rgb(60, 62, 66)
                    };
                    ui.painter().circle_filled(center, 8.0, fill);
                    ui.painter().circle_stroke(center, 8.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 100, 105)));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let color = if captures >= 5 {
                    WIN_HIGHLIGHT
                } else if captures >= 4 {
                    TIMER_WARNING
                } else {
                    TEXT_SECONDARY
                };
                ui.label(RichText::new(format!("{}/5", captures)).size(13.0).color(color));
            });
        });
    }

    /// Render actions section
    fn render_actions_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("ACTIONS").size(10.0).color(TEXT_MUTED));
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            if ui.button("Undo").clicked() {
                self.state.undo();
            }

            if let GameMode::PvP { .. } = self.state.mode {
                if ui.button("Hint").clicked() {
                    self.state.request_suggestion();
                }
            }
        });

        ui.add_space(4.0);
        ui.label(RichText::new(format!("Move #{}", self.state.move_history.len())).size(11.0).color(TEXT_SECONDARY));
    }

    /// Render debug section
    fn render_debug_section(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("AI DEBUG").size(10.0).color(TEXT_MUTED));
        ui.add_space(4.0);

        if let Some(result) = &self.state.last_ai_result {
            ui.label(RichText::new(format!("{:?}", result.search_type)).size(11.0).strong().color(TIMER_NORMAL));
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Score: {}", result.score)).size(10.0).color(TEXT_SECONDARY));
                ui.label(RichText::new(format!("{}ms", result.time_ms)).size(10.0).color(TEXT_MUTED));
                ui.label(RichText::new(format!("{} nodes", result.nodes)).size(10.0).color(TEXT_MUTED));
            });

            if let Some(pos) = result.best_move {
                let col = (b'A' + pos.col) as char;
                let row = 19 - pos.row;
                ui.label(RichText::new(format!("Move: {}{}", col, row)).size(11.0).color(WIN_HIGHLIGHT));
            }
        } else {
            ui.label(RichText::new("No AI data").size(10.0).color(TEXT_MUTED));
        }
    }

    /// Render game over section
    fn render_game_over_section(&mut self, ui: &mut egui::Ui) {
        let result = self.state.game_over.clone().unwrap();
        let is_black = result.winner == Stone::Black;
        let winner = if is_black { "BLACK" } else { "WHITE" };
        let win_type = match result.win_type {
            WinType::FiveInRow => "5-in-a-row",
            WinType::Capture => "10 captures",
        };

        Frame::new()
            .fill(egui::Color32::from_rgb(40, 70, 50))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("GAME OVER").size(14.0).strong().color(WIN_HIGHLIGHT));
                    ui.add_space(8.0);

                    // Winner stone
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), egui::Sense::hover());
                        let center = rect.center();

                        if is_black {
                            ui.painter().circle_filled(center, 14.0, egui::Color32::from_rgb(30, 30, 35));
                            ui.painter().circle_stroke(center, 14.0, egui::Stroke::new(2.0, WIN_HIGHLIGHT));
                        } else {
                            ui.painter().circle_filled(center, 14.0, egui::Color32::from_rgb(240, 240, 245));
                            ui.painter().circle_stroke(center, 14.0, egui::Stroke::new(2.0, WIN_HIGHLIGHT));
                        }

                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(winner).size(16.0).strong().color(TEXT_PRIMARY));
                            ui.label(RichText::new("WINS!").size(12.0).color(WIN_HIGHLIGHT));
                        });
                    });

                    ui.label(RichText::new(format!("by {}", win_type)).size(10.0).color(TEXT_SECONDARY));

                    ui.add_space(10.0);

                    // New Game button - THIS ACTUALLY WORKS NOW
                    if ui.button(RichText::new("New Game").size(14.0).strong()).clicked() {
                        self.new_game_requested = true;
                    }
                });
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
        // Handle new game request
        if self.new_game_requested {
            self.state.reset();
            self.new_game_requested = false;
        }

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
