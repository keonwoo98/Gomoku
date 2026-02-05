//! Main application for the Gomoku GUI

use eframe::egui;
use egui::{CentralPanel, Context, CornerRadius, Frame, RichText, SidePanel, TopBottomPanel, Vec2};

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
            .min_width(240.0)
            .max_width(280.0)
            .frame(Frame::new().fill(egui::Color32::from_rgb(25, 27, 31)))
            .show(ctx, |ui| {
                ui.add_space(12.0);

                // Game title with logo style
                self.render_title_card(ui);
                ui.add_space(12.0);

                // Turn indicator card
                self.render_turn_card(ui);
                ui.add_space(10.0);

                // Timer card
                self.render_timer_card(ui);
                ui.add_space(10.0);

                // Captures card
                self.render_captures_card(ui);
                ui.add_space(10.0);

                // Actions card
                self.render_actions_card(ui);

                // Debug panel (collapsible)
                if self.show_debug {
                    ui.add_space(10.0);
                    self.render_debug_card(ui);
                }

                // Game over overlay
                if let Some(result) = &self.state.game_over.clone() {
                    ui.add_space(10.0);
                    self.render_game_over_card(ui, &result);
                }

                // Status message
                if let Some(msg) = &self.state.message {
                    ui.add_space(10.0);
                    self.render_message_card(ui, msg);
                }
            });
    }

    /// Helper to create a card frame
    fn card_frame() -> Frame {
        Frame::new()
            .fill(egui::Color32::from_rgb(35, 38, 43))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(12.0)
    }

    /// Render title card
    fn render_title_card(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            // Logo stones
            ui.label(RichText::new("●○").size(20.0).color(egui::Color32::from_rgb(180, 180, 185)));
            ui.add_space(4.0);
            ui.label(RichText::new("GOMOKU").size(22.0).strong().color(TEXT_PRIMARY));
        });
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("五目並べ").size(11.0).color(TEXT_MUTED));
        });
    }

    /// Render turn indicator card
    fn render_turn_card(&self, ui: &mut egui::Ui) {
        Self::card_frame().show(ui, |ui| {
            let is_black = self.state.current_turn == Stone::Black;
            let (stone_char, color_name, accent) = if is_black {
                ("●", "BLACK", egui::Color32::from_rgb(70, 70, 75))
            } else {
                ("○", "WHITE", egui::Color32::from_rgb(220, 220, 225))
            };

            ui.horizontal(|ui| {
                // Large stone indicator
                let stone_color = if is_black { TEXT_PRIMARY } else { egui::Color32::from_rgb(30, 30, 35) };

                // Stone circle background
                let (rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 48.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 22.0, accent);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    stone_char,
                    egui::FontId::proportional(28.0),
                    stone_color,
                );

                ui.add_space(12.0);

                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(color_name).size(18.0).strong().color(TEXT_PRIMARY));

                    let status = if self.state.is_ai_thinking() {
                        ("🤔 AI thinking...", TIMER_WARNING)
                    } else if self.state.game_over.is_some() {
                        ("Game Over", WIN_HIGHLIGHT)
                    } else {
                        ("Your turn", TIMER_NORMAL)
                    };
                    ui.label(RichText::new(status.0).size(12.0).color(status.1));
                });
            });
        });
    }

    /// Render timer card
    fn render_timer_card(&self, ui: &mut egui::Ui) {
        Self::card_frame().show(ui, |ui| {
            ui.label(RichText::new("⏱ TIMER").size(10.0).color(TEXT_MUTED));
            ui.add_space(6.0);

            if self.state.is_ai_thinking() {
                if let Some(elapsed) = self.state.ai_thinking_elapsed() {
                    let secs = elapsed.as_secs_f32();
                    let (color, emoji) = if secs < 0.3 {
                        (TIMER_NORMAL, "🟢")
                    } else if secs < 0.5 {
                        (TIMER_WARNING, "🟡")
                    } else {
                        (TIMER_CRITICAL, "🔴")
                    };

                    ui.horizontal(|ui| {
                        ui.label(RichText::new(emoji).size(16.0));
                        ui.label(RichText::new(format!("{:.2}s", secs)).size(28.0).strong().color(color));
                    });
                }
            } else {
                let elapsed = self.state.move_timer.elapsed();
                ui.label(RichText::new(format!("{:.1}s", elapsed.as_secs_f32())).size(24.0).color(TEXT_PRIMARY));
            }

            if let Some(ai_time) = self.state.move_timer.ai_thinking_time {
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Last AI: {:.3}s", ai_time.as_secs_f32())).size(10.0).color(TEXT_SECONDARY));
            }
        });
    }

    /// Render captures card
    fn render_captures_card(&self, ui: &mut egui::Ui) {
        Self::card_frame().show(ui, |ui| {
            ui.label(RichText::new("🎯 CAPTURES").size(10.0).color(TEXT_MUTED));
            ui.add_space(8.0);

            // Black captures
            self.render_capture_row(ui, true, self.state.board.black_captures);
            ui.add_space(6.0);

            // White captures
            self.render_capture_row(ui, false, self.state.board.white_captures);
        });
    }

    /// Render a single capture row with stone icons
    fn render_capture_row(&self, ui: &mut egui::Ui, is_black: bool, captures: u8) {
        let (symbol, filled_color, empty_color) = if is_black {
            ("●", egui::Color32::from_rgb(60, 60, 65), egui::Color32::from_rgb(40, 42, 46))
        } else {
            ("○", egui::Color32::from_rgb(200, 200, 205), egui::Color32::from_rgb(60, 62, 66))
        };

        ui.horizontal(|ui| {
            // Show 5 stone indicators
            for i in 0..5u8 {
                let color = if i < captures {
                    if captures >= 4 { TIMER_WARNING } else { filled_color }
                } else {
                    empty_color
                };
                ui.label(RichText::new(symbol).size(18.0).color(color));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let text = if captures >= 5 {
                    RichText::new("WIN!").size(14.0).strong().color(WIN_HIGHLIGHT)
                } else if captures >= 4 {
                    RichText::new(format!("{}/5", captures)).size(14.0).strong().color(TIMER_WARNING)
                } else {
                    RichText::new(format!("{}/5", captures)).size(14.0).color(TEXT_SECONDARY)
                };
                ui.label(text);
            });
        });
    }

    /// Render actions card
    fn render_actions_card(&mut self, ui: &mut egui::Ui) {
        Self::card_frame().show(ui, |ui| {
            ui.label(RichText::new("⚡ ACTIONS").size(10.0).color(TEXT_MUTED));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                // Styled buttons
                let btn_frame = Frame::new()
                    .fill(egui::Color32::from_rgb(50, 53, 58))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(8.0);

                btn_frame.show(ui, |ui| {
                    if ui.add(egui::Label::new(RichText::new("↩ Undo").size(12.0).color(TEXT_PRIMARY)).sense(egui::Sense::click())).clicked() {
                        self.state.undo();
                    }
                });

                ui.add_space(4.0);

                if let GameMode::PvP { .. } = self.state.mode {
                    btn_frame.show(ui, |ui| {
                        if ui.add(egui::Label::new(RichText::new("💡 Hint").size(12.0).color(TEXT_PRIMARY)).sense(egui::Sense::click())).clicked() {
                            self.state.request_suggestion();
                        }
                    });
                }
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Move #{}", self.state.move_history.len())).size(11.0).color(TEXT_SECONDARY));
            });
        });
    }

    /// Render debug card
    fn render_debug_card(&self, ui: &mut egui::Ui) {
        Frame::new()
            .fill(egui::Color32::from_rgb(30, 33, 38))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.label(RichText::new("🔧 AI DEBUG").size(10.0).color(TEXT_MUTED));
                ui.add_space(6.0);

                if let Some(result) = &self.state.last_ai_result {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{:?}", result.search_type)).size(11.0).strong().color(TIMER_NORMAL));
                            ui.label(RichText::new(format!("Score: {}", result.score)).size(10.0).color(TEXT_SECONDARY));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("{}ms", result.time_ms)).size(10.0).color(TEXT_SECONDARY));
                                ui.label(RichText::new(format!("{} nodes", result.nodes)).size(10.0).color(TEXT_MUTED));
                            });
                        });
                    });

                    if let Some(pos) = result.best_move {
                        let col = (b'A' + pos.col) as char;
                        let row = 19 - pos.row;
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("→ {}{}", col, row)).size(12.0).strong().color(WIN_HIGHLIGHT));
                    }
                } else {
                    ui.label(RichText::new("Waiting for AI...").size(10.0).color(TEXT_MUTED));
                }
            });
    }

    /// Render game over card
    fn render_game_over_card(&self, ui: &mut egui::Ui, result: &GameResult) {
        let (winner, symbol, accent) = if result.winner == Stone::Black {
            ("BLACK", "●", egui::Color32::from_rgb(70, 70, 75))
        } else {
            ("WHITE", "○", egui::Color32::from_rgb(220, 220, 225))
        };
        let win_type = match result.win_type {
            WinType::FiveInRow => "5-in-a-row",
            WinType::Capture => "10 captures",
        };

        Frame::new()
            .fill(egui::Color32::from_rgb(45, 80, 55))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("🎉 GAME OVER").size(12.0).color(egui::Color32::from_rgb(180, 255, 180)));
                    ui.add_space(8.0);

                    // Winner display
                    ui.horizontal(|ui| {
                        ui.add_space(ui.available_width() / 2.0 - 60.0);
                        ui.label(RichText::new(symbol).size(32.0).color(accent));
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(winner).size(18.0).strong().color(TEXT_PRIMARY));
                            ui.label(RichText::new("WINS!").size(14.0).color(WIN_HIGHLIGHT));
                        });
                    });

                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("by {}", win_type)).size(11.0).color(TEXT_SECONDARY));

                    ui.add_space(12.0);

                    // New game button
                    Frame::new()
                        .fill(egui::Color32::from_rgb(60, 100, 70))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            if ui.add(egui::Label::new(
                                RichText::new("🔄 New Game").size(14.0).strong().color(TEXT_PRIMARY)
                            ).sense(egui::Sense::click())).clicked() {
                                // Will be handled
                            }
                        });
                });
            });
    }

    /// Render status message card
    fn render_message_card(&self, ui: &mut egui::Ui, msg: &str) {
        Frame::new()
            .fill(egui::Color32::from_rgb(80, 60, 30))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚠").size(14.0));
                    ui.add_space(4.0);
                    ui.label(RichText::new(msg).size(11.0).color(TEXT_PRIMARY));
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
