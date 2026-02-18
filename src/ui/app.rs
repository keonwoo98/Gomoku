//! Main application for the Gomoku GUI

use eframe::egui;
use egui::{CentralPanel, Context, CornerRadius, Frame, RichText, ScrollArea, SidePanel, TopBottomPanel, Vec2};

use crate::{Pos, Stone};
use super::board_view::BoardView;
use super::game_state::{GameMode, GameState, OpeningRule, WinType};
use super::theme::*;

/// Main Gomoku application
pub struct GomokuApp {
    state: GameState,
    board_view: BoardView,
    show_debug: bool,
    new_game_requested: bool,
}

impl Default for GomokuApp {
    fn default() -> Self {
        Self {
            state: GameState::new(GameMode::default()),
            board_view: BoardView::default(),
            show_debug: true,
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
                    ui.menu_button("New Game (PvE - Black)", |ui| {
                        for (label, rule) in [("Standard", OpeningRule::Standard), ("Pro", OpeningRule::Pro), ("Swap", OpeningRule::Swap)] {
                            if ui.button(label).clicked() {
                                self.state = GameState::with_opening_rule(
                                    GameMode::PvE { human_color: Stone::Black }, rule);
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("New Game (PvE - White)", |ui| {
                        for (label, rule) in [("Standard", OpeningRule::Standard), ("Pro", OpeningRule::Pro), ("Swap", OpeningRule::Swap)] {
                            if ui.button(label).clicked() {
                                self.state = GameState::with_opening_rule(
                                    GameMode::PvE { human_color: Stone::White }, rule);
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("New Game (PvP)", |ui| {
                        for (label, rule) in [("Standard", OpeningRule::Standard), ("Pro", OpeningRule::Pro), ("Swap", OpeningRule::Swap)] {
                            if ui.button(label).clicked() {
                                self.state = GameState::with_opening_rule(
                                    GameMode::PvP { show_suggestions: false }, rule);
                                ui.close_menu();
                            }
                        }
                    });
                    ui.menu_button("New Game (AI vs AI)", |ui| {
                        for (label, rule) in [("Standard", OpeningRule::Standard), ("Pro", OpeningRule::Pro), ("Swap", OpeningRule::Swap)] {
                            if ui.button(label).clicked() {
                                self.state = GameState::with_opening_rule(
                                    GameMode::AiVsAi, rule);
                                ui.close_menu();
                            }
                        }
                    });
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
                    // Show current mode + opening rule
                    let rule_str = match self.state.opening_rule {
                        OpeningRule::Standard => "",
                        OpeningRule::Pro => " [Pro]",
                        OpeningRule::Swap => " [Swap]",
                    };
                    let mode_text = match self.state.mode {
                        GameMode::PvE { human_color } => {
                            format!("PvE - You: {}{}", if human_color == Stone::Black { "Black" } else { "White" }, rule_str)
                        }
                        GameMode::PvP { .. } => format!("PvP - Hotseat{}", rule_str),
                        GameMode::AiVsAi => format!("AI vs AI - Spectator{}", rule_str),
                    };
                    ui.label(mode_text);
                });
            });
        });
    }

    /// Helper: render a card-style section with optional header
    fn render_card(ui: &mut egui::Ui, header: Option<(&str, egui::Color32)>, add_contents: impl FnOnce(&mut egui::Ui)) {
        Frame::new()
            .fill(PANEL_CARD)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some((title, color)) = header {
                    ui.horizontal(|ui| {
                        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(3.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, CornerRadius::same(1), color);
                        ui.add_space(3.0);
                        ui.label(RichText::new(title).size(11.0).strong().color(color));
                    });
                    ui.add_space(5.0);
                }
                add_contents(ui);
            });
    }

    /// Render the side panel with game info and debug
    fn render_side_panel(&mut self, ctx: &Context) {
        SidePanel::right("info_panel")
            .min_width(270.0)
            .max_width(310.0)
            .frame(Frame::new()
                .fill(PANEL_BG)
                .inner_margin(10.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    // Title
                    ui.vertical_centered(|ui| {
                        ui.add_space(1.0);
                        ui.label(RichText::new("GOMOKU").size(15.0).strong().color(ACCENT_BLUE));
                        ui.add_space(1.0);
                    });

                    ui.add_space(4.0);

                    // Game over (shown at top when game is over for visibility)
                    if self.state.game_over.is_some() {
                        self.render_game_over_section(ui);
                        ui.add_space(4.0);
                    }

                    // Turn + Timer + Actions (combined)
                    self.render_turn_section(ui);
                    ui.add_space(4.0);

                    // Message (invalid move feedback)
                    if let Some(msg) = &self.state.message {
                        Frame::new()
                            .fill(egui::Color32::from_rgb(100, 30, 30))
                            .corner_radius(CornerRadius::same(5))
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new(msg.as_str()).size(10.0).strong().color(egui::Color32::from_rgb(255, 200, 80)));
                                });
                            });
                        ui.add_space(4.0);
                    }

                    // Captures
                    self.render_captures_section(ui);
                    ui.add_space(4.0);

                    // Debug (if enabled)
                    if self.show_debug {
                        self.render_debug_section(ui);
                    }

                    ui.add_space(4.0);
                });
            });
    }

    /// Render turn indicator showing both sides, with active turn highlighted
    fn render_turn_section(&mut self, ui: &mut egui::Ui) {
        let active_black = self.state.current_turn == Stone::Black;

        Self::render_card(ui, None, |ui| {
            // Black row
            Self::render_turn_row(ui, true, active_black, &self.state);
            ui.add_space(3.0);
            // White row
            Self::render_turn_row(ui, false, !active_black, &self.state);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("#{}", self.state.move_history.len())).size(10.0).color(TEXT_MUTED));
                ui.add_space(3.0);

                if ui.small_button("Undo").clicked() {
                    self.state.undo();
                }
                if ui.small_button("Redo").clicked() {
                    self.state.redo();
                }

                if let GameMode::PvP { .. } = self.state.mode {
                    if ui.small_button("Hint").clicked() {
                        self.state.request_suggestion();
                    }
                }

            });
        });
    }

    /// Render a single turn row (Black or White)
    fn render_turn_row(ui: &mut egui::Ui, is_black: bool, is_active: bool, state: &GameState) {
        let color_name = if is_black { "BLACK" } else { "WHITE" };
        let dimmed = !is_active;
        let name_color = if dimmed { TEXT_MUTED } else { TEXT_PRIMARY };

        ui.horizontal(|ui| {
            // Stone icon
            let (rect, _) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), egui::Sense::hover());
            let center = rect.center();
            let alpha = if dimmed { 80u8 } else { 255 };

            if is_black {
                ui.painter().circle_filled(center + Vec2::new(0.8, 0.8), 10.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, alpha / 5));
                ui.painter().circle_filled(center, 10.0, egui::Color32::from_rgba_unmultiplied(30, 30, 35, alpha));
                let ring_a = if dimmed { 40 } else { 255 };
                ui.painter().circle_stroke(center, 10.0, egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(60, 60, 65, ring_a)));
            } else {
                ui.painter().circle_filled(center + Vec2::new(0.8, 0.8), 10.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, alpha / 8));
                ui.painter().circle_filled(center, 10.0, egui::Color32::from_rgba_unmultiplied(245, 245, 248, alpha));
                let ring_a = if dimmed { 60 } else { 255 };
                ui.painter().circle_stroke(center, 10.0, egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(180, 180, 185, ring_a)));
            }

            ui.add_space(2.0);

            // Name + status
            ui.vertical(|ui| {
                ui.label(RichText::new(color_name).size(13.0).strong().color(name_color));
                if is_active {
                    let (status_text, status_color) = if state.is_ai_thinking() {
                        ("AI thinking...", TIMER_WARNING)
                    } else if state.game_over.is_some() {
                        ("Game Over", WIN_HIGHLIGHT)
                    } else {
                        ("to move", TIMER_NORMAL)
                    };
                    ui.label(RichText::new(status_text).size(9.0).color(status_color));
                }
            });

            // Timer (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if is_active {
                    // Active side: live timer
                    if state.is_ai_thinking() {
                        if let Some(elapsed) = state.ai_thinking_elapsed() {
                            let secs = elapsed.as_secs_f32();
                            let color = if secs < 0.3 {
                                TIMER_NORMAL
                            } else if secs < 0.5 {
                                TIMER_WARNING
                            } else {
                                TIMER_CRITICAL
                            };
                            ui.label(RichText::new(format!("{:.2}s", secs)).size(18.0).strong().color(color));
                        }
                    } else {
                        let elapsed = state.move_timer.elapsed();
                        ui.label(RichText::new(format!("{:.1}s", elapsed.as_secs_f32())).size(15.0).color(TEXT_SECONDARY));
                    }
                } else {
                    // Inactive side: show engine time from last move
                    let idx = if is_black { 0 } else { 1 };
                    if let Some(result) = &state.last_ai_result[idx] {
                        let ms = result.time_ms;
                        let text = if ms >= 1000 {
                            format!("{:.1}s", ms as f64 / 1000.0)
                        } else {
                            format!("{}ms", ms)
                        };
                        ui.label(RichText::new(text).size(13.0).color(TEXT_MUTED));
                    } else if let Some(dur) = state.last_move_time[idx] {
                        // Human move (PvP): show wallclock
                        let ms = dur.as_millis();
                        let text = if ms >= 1000 {
                            format!("{:.1}s", dur.as_secs_f32())
                        } else {
                            format!("{}ms", ms)
                        };
                        ui.label(RichText::new(text).size(13.0).color(TEXT_MUTED));
                    }
                }
            });
        });
    }

    /// Render captures section with painted stones
    fn render_captures_section(&self, ui: &mut egui::Ui) {
        Self::render_card(ui, Some(("CAPTURES", TEXT_MUTED)), |ui| {
            self.render_capture_row_painted(ui, true, self.state.board.black_captures);
            ui.add_space(4.0);
            self.render_capture_row_painted(ui, false, self.state.board.white_captures);
        });
    }

    /// Render capture row with painted circles
    fn render_capture_row_painted(&self, ui: &mut egui::Ui, is_black: bool, captures: u8) {
        ui.horizontal(|ui| {
            // Fixed-width label for consistent alignment
            let (label_rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 20.0), egui::Sense::hover());
            let label = if is_black { "B" } else { "W" };
            let label_color = if is_black { egui::Color32::from_rgb(140, 140, 150) } else { egui::Color32::from_rgb(200, 200, 210) };
            ui.painter().text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(11.0),
                label_color,
            );
            ui.add_space(2.0);

            for i in 0..5u8 {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), egui::Sense::hover());
                let center = rect.center();
                let filled = i < captures;
                let near_win = captures >= 4 && filled;

                if filled {
                    let fill = if near_win {
                        egui::Color32::from_rgb(255, 60, 60)
                    } else if is_black {
                        egui::Color32::from_rgb(25, 25, 30)
                    } else {
                        egui::Color32::from_rgb(250, 250, 252)
                    };
                    ui.painter().circle_filled(center + Vec2::new(0.8, 0.8), 9.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 40));
                    ui.painter().circle_filled(center, 9.0, fill);
                    let ring_color = if near_win {
                        egui::Color32::from_rgb(255, 200, 100)
                    } else if is_black {
                        egui::Color32::from_rgb(80, 80, 90)
                    } else {
                        egui::Color32::from_rgb(190, 190, 200)
                    };
                    ui.painter().circle_stroke(center, 9.0, egui::Stroke::new(1.5, ring_color));
                } else {
                    ui.painter().circle_stroke(center, 9.0, egui::Stroke::new(1.0, ACCENT_DIM));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let color = if captures >= 5 {
                    WIN_HIGHLIGHT
                } else if captures >= 4 {
                    TIMER_CRITICAL
                } else {
                    TEXT_SECONDARY
                };
                ui.label(RichText::new(format!("{}/5", captures)).size(13.0).strong().color(color));
            });
        });
    }

    /// Helper: render a key-value row in a grid
    fn grid_row(ui: &mut egui::Ui, label: &str, value: &str, value_color: egui::Color32) {
        ui.label(RichText::new(label).size(11.0).color(TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(11.0).color(value_color));
        });
        ui.end_row();
    }

    /// Render debug section with detailed AI search statistics for both sides
    fn render_debug_section(&self, ui: &mut egui::Ui) {
        for (idx, color_name) in [(0usize, "BLACK"), (1, "WHITE")] {
            let result = &self.state.last_ai_result[idx];
            let stats = &self.state.ai_stats[idx];

            // Skip sides with no data
            if result.is_none() && stats.move_count == 0 {
                continue;
            }

            // Last move card per side
            let header = format!("{} LAST MOVE", color_name);
            Self::render_card(ui, Some((&header, ACCENT_BLUE)), |ui| {
                if let Some(result) = result {
                    let (type_str, type_color) = match result.search_type {
                        crate::engine::SearchType::ImmediateWin => ("Immediate Win", WIN_HIGHLIGHT),
                        crate::engine::SearchType::VCF => ("VCF", WIN_HIGHLIGHT),
                        crate::engine::SearchType::Defense => ("Defense", TIMER_CRITICAL),
                        crate::engine::SearchType::AlphaBeta => ("Alpha-Beta", TIMER_NORMAL),
                    };

                    ui.horizontal(|ui| {
                        Frame::new()
                            .fill(PANEL_CARD_ACCENT)
                            .corner_radius(CornerRadius::same(3))
                            .inner_margin(egui::Margin::symmetric(7, 3))
                            .show(ui, |ui| {
                                ui.label(RichText::new(type_str).size(11.0).strong().color(type_color));
                            });

                        if let Some(pos) = result.best_move {
                            let notation = crate::engine::pos_to_notation(pos);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(notation).size(13.0).strong().color(TEXT_PRIMARY));
                            });
                        }
                    });

                    ui.add_space(2.0);

                    let (score_text, score_color) = if result.score >= 999_900 {
                        ("+WIN".to_string(), WIN_HIGHLIGHT)
                    } else if result.score <= -999_900 {
                        ("-LOSE".to_string(), TIMER_CRITICAL)
                    } else if result.score > 50_000 {
                        (format!("+{}", result.score), WIN_HIGHLIGHT)
                    } else if result.score < -50_000 {
                        (format!("{}", result.score), TIMER_CRITICAL)
                    } else if result.score > 0 {
                        (format!("+{}", result.score), TIMER_NORMAL)
                    } else {
                        (format!("{}", result.score), TEXT_SECONDARY)
                    };

                    let grid_id = format!("last_move_grid_{}", idx);
                    egui::Grid::new(grid_id)
                        .num_columns(2)
                        .min_col_width(ui.available_width() / 2.0 - 8.0)
                        .spacing([8.0, 2.0])
                        .show(ui, |ui| {
                            Self::grid_row(ui, "Score", &score_text, score_color);

                            if result.depth > 0 {
                                let time_str = if result.time_ms >= 1000 {
                                    format!("{:.2}s", result.time_ms as f64 / 1000.0)
                                } else {
                                    format!("{}ms", result.time_ms)
                                };
                                let time_color = if result.time_ms > 500 {
                                    TIMER_CRITICAL
                                } else if result.time_ms > 200 {
                                    TIMER_WARNING
                                } else {
                                    TIMER_NORMAL
                                };
                                Self::grid_row(ui, "Time", &time_str, time_color);

                                let depth_color = if result.depth >= 10 {
                                    TIMER_NORMAL
                                } else if result.depth >= 6 {
                                    TIMER_WARNING
                                } else {
                                    TEXT_SECONDARY
                                };
                                Self::grid_row(ui, "Depth", &format!("{}", result.depth), depth_color);

                                let nodes_str = if result.nodes >= 1_000_000 {
                                    format!("{:.1}M", result.nodes as f64 / 1_000_000.0)
                                } else if result.nodes >= 1_000 {
                                    format!("{:.1}K", result.nodes as f64 / 1_000.0)
                                } else {
                                    format!("{}", result.nodes)
                                };
                                Self::grid_row(ui, "Nodes", &nodes_str, TEXT_SECONDARY);

                                if result.nps > 0 {
                                    Self::grid_row(ui, "Speed", &format!("{} kN/s", result.nps), TEXT_SECONDARY);
                                }
                                if result.tt_usage > 0 {
                                    Self::grid_row(ui, "TT Hit", &format!("{}%", result.tt_usage), TEXT_SECONDARY);
                                }
                            } else {
                                Self::grid_row(ui, "Detection", "Instant", TIMER_NORMAL);

                                let last_search = stats.move_depths.iter().zip(stats.move_times.iter())
                                    .rev()
                                    .find(|(&d, _)| d > 0);
                                if let Some((&depth, &time)) = last_search {
                                    let prev_str = format!("d{}, {}ms", depth, time);
                                    Self::grid_row(ui, "Prev Search", &prev_str, TEXT_MUTED);
                                }
                            }
                        });
                } else {
                    ui.label(RichText::new("No data yet").size(11.0).color(TEXT_MUTED));
                }
            });

            // Stats card per side
            if stats.move_count > 0 {
                ui.add_space(4.0);
                let stats_header = format!("{} STATS", color_name);
                Self::render_card(ui, Some((&stats_header, ACCENT_BLUE)), |ui| {
                    let grid_id = format!("ai_stats_grid_{}", idx);
                    egui::Grid::new(grid_id)
                        .num_columns(2)
                        .min_col_width(ui.available_width() / 2.0 - 8.0)
                        .spacing([8.0, 2.0])
                        .show(ui, |ui| {
                            let search_count = stats.move_depths.iter().filter(|&&d| d > 0).count();
                            Self::grid_row(ui, "AI Moves", &format!("{} ({} search)", stats.move_count, search_count), TEXT_PRIMARY);

                            let avg = stats.avg_time_ms();
                            let avg_str = if avg >= 1000.0 {
                                format!("{:.2}s", avg / 1000.0)
                            } else {
                                format!("{:.0}ms", avg)
                            };
                            let avg_color = if avg > 500.0 {
                                TIMER_CRITICAL
                            } else if avg > 200.0 {
                                TIMER_WARNING
                            } else {
                                TIMER_NORMAL
                            };
                            Self::grid_row(ui, "Avg Time", &avg_str, avg_color);

                            let (search_min, search_max) = stats.search_time_range();
                            Self::grid_row(ui, "Time Range", &format!("{} - {}ms", search_min, search_max), TEXT_SECONDARY);

                            Self::grid_row(ui, "Avg Depth", &format!("{:.1}", stats.avg_depth()), TEXT_SECONDARY);

                            let max_depth_color = if stats.max_depth >= 10 { TIMER_NORMAL } else { TEXT_SECONDARY };
                            Self::grid_row(ui, "Max Depth", &format!("{}", stats.max_depth), max_depth_color);

                            let total_str = if stats.total_nodes >= 1_000_000 {
                                format!("{:.1}M", stats.total_nodes as f64 / 1_000_000.0)
                            } else if stats.total_nodes >= 1_000 {
                                format!("{:.1}K", stats.total_nodes as f64 / 1_000.0)
                            } else {
                                format!("{}", stats.total_nodes)
                            };
                            Self::grid_row(ui, "Total Nodes", &total_str, TEXT_SECONDARY);

                            if stats.avg_nps() > 0 {
                                Self::grid_row(ui, "Avg Speed", &format!("{} kN/s", stats.avg_nps()), TEXT_SECONDARY);
                            }
                        });
                });
            }

            ui.add_space(4.0);
        }
    }

    /// Render game over section
    fn render_game_over_section(&mut self, ui: &mut egui::Ui) {
        let Some(result) = self.state.game_over.clone() else {
            return;
        };
        let is_black = result.winner == Stone::Black;
        let winner = if is_black { "BLACK" } else { "WHITE" };
        let win_type = match result.win_type {
            WinType::FiveInRow => "5-in-a-row",
            WinType::Capture => "10 captures",
        };

        Frame::new()
            .fill(egui::Color32::from_rgb(30, 60, 40))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 140, 70)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                // Winner info + New Game button (separate rows to avoid overlap)
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), egui::Sense::hover());
                    let center = rect.center();
                    let stone_color = if is_black {
                        egui::Color32::from_rgb(30, 30, 35)
                    } else {
                        egui::Color32::from_rgb(245, 245, 248)
                    };
                    ui.painter().circle_filled(center, 9.0, stone_color);
                    ui.painter().circle_stroke(center, 9.0, egui::Stroke::new(1.5, WIN_HIGHLIGHT));

                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{} WINS!", winner)).size(14.0).strong().color(TEXT_PRIMARY));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("New Game").clicked() {
                            self.new_game_requested = true;
                        }
                    });
                });
                // Win details on separate line
                let move_count = self.state.move_history.len();
                let last_info = if let Some(pos) = self.state.last_move {
                    let notation = crate::engine::pos_to_notation(pos);
                    format!("by {} at {} (move #{})", win_type, notation, move_count)
                } else {
                    format!("by {}", win_type)
                };
                ui.label(RichText::new(last_info).size(10.0).color(TEXT_SECONDARY));

                // Review navigation - compact inline
                let total = self.state.move_history.len();
                let current = self.state.review_index.unwrap_or(total);
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let s = Vec2::new(24.0, 18.0);

                        if ui.add_sized(s, egui::Button::new(
                            RichText::new("<<").size(10.0).color(TEXT_SECONDARY)
                        )).clicked() {
                            self.state.review_index = Some(0);
                        }
                        if ui.add_sized(s, egui::Button::new(
                            RichText::new("<").size(10.0).color(TEXT_SECONDARY)
                        )).clicked() {
                            self.state.review_prev();
                        }

                        ui.label(RichText::new(format!(" {}/{} ", current, total))
                            .size(10.0).color(TEXT_SECONDARY));

                        if ui.add_sized(s, egui::Button::new(
                            RichText::new(">").size(10.0).color(TEXT_SECONDARY)
                        )).clicked() {
                            self.state.review_next();
                        }
                        if ui.add_sized(s, egui::Button::new(
                            RichText::new(">>").size(10.0).color(TEXT_SECONDARY)
                        )).clicked() {
                            self.state.review_index = None;
                        }
                    });
                });
            });
    }

    /// Render the main board
    fn render_board(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            // Set board area background
            ui.style_mut().visuals.panel_fill = egui::Color32::from_rgb(40, 42, 46);

            // In review mode, show a temporary board at the review index
            let (board_ref, last_move, winning_line) = if let Some(idx) = self.state.review_index {
                let (review_board, review_last) = self.state.build_review_board(idx);
                // Store temporarily for rendering
                (review_board, review_last, None)
            } else {
                let wl = self.state.game_over.as_ref().and_then(|r| r.winning_line);
                (self.state.board.clone(), self.state.last_move, wl)
            };

            // Center board vertically in available space
            let available = ui.available_size();
            let board_size = available.x.min(available.y);
            let pad_y = (available.y - board_size).max(0.0) / 2.0;
            ui.add_space(pad_y);

            // Pro rule restriction closure for hover validation
            let opening_rule = self.state.opening_rule;
            let move_count = self.state.move_history.len();
            let pro_invalid: Option<Box<dyn Fn(Pos) -> bool>> = if opening_rule == OpeningRule::Pro {
                Some(Box::new(move |pos: Pos| {
                    let move_num = move_count + 1;
                    if move_num == 1 && pos != Pos::new(9, 9) {
                        return true;
                    }
                    if move_num == 3 {
                        let center = 9i32;
                        let dr = (i32::from(pos.row) - center).abs();
                        let dc = (i32::from(pos.col) - center).abs();
                        if dr.max(dc) < 3 {
                            return true;
                        }
                    }
                    false
                }))
            } else {
                None
            };

            let clicked = self.board_view.show(
                ui,
                &board_ref,
                self.state.current_turn,
                last_move,
                self.state.suggested_move,
                winning_line,
                self.state.game_over.is_some() && !self.state.is_reviewing(),
                self.state.capture_animation.as_ref(),
                pro_invalid.as_ref().map(|f| f.as_ref()),
            );

            // Handle click (only when not reviewing and no swap pending)
            if !self.state.is_reviewing() && !self.state.swap_pending {
                if let Some(pos) = clicked {
                    if let Err(msg) = self.state.try_place_stone(pos) {
                        self.state.message = Some(msg);
                    }
                }
            }
        });
    }

    /// Render swap rule dialog overlay
    fn render_swap_dialog(&mut self, ctx: &Context) {
        egui::Area::new(egui::Id::new("swap_dialog"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(egui::Color32::from_rgb(35, 40, 50))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(24, 18))
                    .stroke(egui::Stroke::new(2.0, ACCENT_BLUE))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("Swap Rule").size(16.0).strong().color(ACCENT_BLUE));
                            ui.add_space(8.0);
                            ui.label(RichText::new("Do you want to swap colors?").size(13.0).color(TEXT_PRIMARY));
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("  Yes, Swap  ").size(13.0)).clicked() {
                                    self.state.execute_swap();
                                }
                                ui.add_space(12.0);
                                if ui.button(RichText::new("  No, Continue  ").size(13.0)).clicked() {
                                    self.state.decline_swap();
                                }
                            });
                        });
                    });
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

            // R - Redo
            if i.key_pressed(egui::Key::R) {
                self.state.redo();
            }

            // Left/Right arrows - Review mode (after game over)
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.state.review_prev();
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                self.state.review_next();
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

        // Escape - Quit (outside input closure to avoid deadlock)
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Check AI result
        self.state.check_ai_result();

        // Clean up completed capture animations
        if let Some(animation) = &self.state.capture_animation {
            if animation.is_complete() {
                self.state.capture_animation = None;
            }
        }

        // Start AI thinking if needed (not during swap decision)
        if self.state.is_ai_turn() && !self.state.is_ai_thinking() && self.state.game_over.is_none() && !self.state.swap_pending {
            self.state.start_ai_thinking();
        }

        // Auto-decide swap for AI in PvE/AiVsAi mode
        if self.state.swap_pending {
            match self.state.mode {
                GameMode::PvE { human_color } => {
                    if self.state.current_turn != human_color {
                        // AI decides: always swap (takes initiative)
                        self.state.execute_swap();
                    }
                }
                GameMode::AiVsAi => {
                    // AI auto-decides: always decline swap
                    self.state.decline_swap();
                }
                _ => {}
            }
        }

        // Render UI
        self.render_menu_bar(ctx);
        self.render_side_panel(ctx);
        self.render_board(ctx);

        // Swap dialog overlay (only for human decision)
        if self.state.swap_pending {
            self.render_swap_dialog(ctx);
        }

        // Always repaint while game is in progress (live timer), plus animations/messages
        let game_in_progress = self.state.game_over.is_none();
        if game_in_progress || self.state.capture_animation.is_some() || self.state.message.is_some() {
            ctx.request_repaint();
        }
    }
}
