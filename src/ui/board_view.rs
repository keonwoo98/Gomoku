//! Board rendering for the Gomoku GUI

use crate::{Pos, Stone, BOARD_SIZE};
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Sense, Stroke, Vec2};

use super::theme::*;

/// Board view handles rendering and input for the game board
pub struct BoardView {
    /// Cached cell size for coordinate calculations
    cell_size: f32,
    /// Board drawing area
    board_rect: Rect,
}

impl Default for BoardView {
    fn default() -> Self {
        Self {
            cell_size: 30.0,
            board_rect: Rect::NOTHING,
        }
    }
}

impl BoardView {
    /// Render the board and return click position if any
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        board: &crate::Board,
        current_turn: Stone,
        last_move: Option<Pos>,
        suggested_move: Option<Pos>,
        winning_line: Option<[Pos; 5]>,
        game_over: bool,
    ) -> Option<Pos> {
        let available_size = ui.available_size();

        // Calculate board size to fit available space
        let board_size = available_size.x.min(available_size.y) - 20.0;
        self.cell_size = (board_size - 2.0 * BOARD_MARGIN) / (BOARD_SIZE as f32 - 1.0);

        let (response, painter) = ui.allocate_painter(
            Vec2::new(board_size, board_size),
            Sense::click(),
        );

        self.board_rect = response.rect;

        // Draw board background
        painter.rect_filled(self.board_rect, CornerRadius::same(4), BOARD_BG);

        // Draw grid lines
        self.draw_grid(&painter);

        // Draw star points
        self.draw_star_points(&painter);

        // Draw coordinate labels
        self.draw_coordinates(&painter);

        // Draw placed stones
        self.draw_stones(&painter, board);

        // Draw last move marker
        if let Some(pos) = last_move {
            self.draw_last_move_marker(&painter, pos);
        }

        // Draw winning line highlight
        if let Some(line) = winning_line {
            self.draw_winning_line(&painter, &line);
        }

        // Draw suggested move
        if let Some(pos) = suggested_move {
            self.draw_suggestion(&painter, pos, current_turn);
        }

        // Handle hover preview and click
        let mut clicked_pos = None;

        if !game_over {
            if let Some(pointer_pos) = response.hover_pos() {
                if let Some(board_pos) = self.screen_to_board(pointer_pos) {
                    let is_valid = board.get(board_pos) == Stone::Empty
                        && crate::rules::is_valid_move(board, board_pos, current_turn);

                    // Draw hover preview
                    let hover_color = if is_valid {
                        super::theme::hover_valid()
                    } else {
                        super::theme::hover_invalid()
                    };
                    self.draw_hover_preview(&painter, board_pos, current_turn, is_valid, hover_color);

                    // Check for click
                    if response.clicked() && is_valid {
                        clicked_pos = Some(board_pos);
                    }
                }
            }
        }

        clicked_pos
    }

    /// Draw the 19x19 grid lines
    fn draw_grid(&self, painter: &Painter) {
        let stroke = Stroke::new(GRID_LINE_WIDTH, GRID_LINE);

        for i in 0..BOARD_SIZE {
            let offset = BOARD_MARGIN + i as f32 * self.cell_size;

            // Vertical line
            let start = self.board_rect.min + Vec2::new(offset, BOARD_MARGIN);
            let end = self.board_rect.min + Vec2::new(offset, BOARD_MARGIN + (BOARD_SIZE as f32 - 1.0) * self.cell_size);
            painter.line_segment([start, end], stroke);

            // Horizontal line
            let start = self.board_rect.min + Vec2::new(BOARD_MARGIN, offset);
            let end = self.board_rect.min + Vec2::new(BOARD_MARGIN + (BOARD_SIZE as f32 - 1.0) * self.cell_size, offset);
            painter.line_segment([start, end], stroke);
        }
    }

    /// Draw star points (hoshi)
    fn draw_star_points(&self, painter: &Painter) {
        for (row, col) in STAR_POINTS {
            let center = self.board_to_screen(Pos::new(row, col));
            painter.circle_filled(center, STAR_POINT_RADIUS, STAR_POINT);
        }
    }

    /// Draw coordinate labels (A-S, 1-19)
    fn draw_coordinates(&self, painter: &Painter) {
        let font = egui::FontId::proportional(12.0);

        // Column labels (A-S)
        for col in 0..BOARD_SIZE {
            let letter = (b'A' + col as u8) as char;
            let x = self.board_rect.min.x + BOARD_MARGIN + col as f32 * self.cell_size;

            // Top
            let pos = Pos2::new(x - 4.0, self.board_rect.min.y + 8.0);
            painter.text(pos, egui::Align2::CENTER_CENTER, letter, font.clone(), GRID_LINE);

            // Bottom
            let pos = Pos2::new(x - 4.0, self.board_rect.max.y - 12.0);
            painter.text(pos, egui::Align2::CENTER_CENTER, letter, font.clone(), GRID_LINE);
        }

        // Row labels (19-1, displayed top to bottom)
        for row in 0..BOARD_SIZE {
            let num = BOARD_SIZE - row;
            let y = self.board_rect.min.y + BOARD_MARGIN + row as f32 * self.cell_size;

            // Left
            let pos = Pos2::new(self.board_rect.min.x + 12.0, y);
            painter.text(pos, egui::Align2::CENTER_CENTER, format!("{}", num), font.clone(), GRID_LINE);

            // Right
            let pos = Pos2::new(self.board_rect.max.x - 12.0, y);
            painter.text(pos, egui::Align2::CENTER_CENTER, format!("{}", num), font.clone(), GRID_LINE);
        }
    }

    /// Draw all placed stones
    fn draw_stones(&self, painter: &Painter, board: &crate::Board) {
        for row in 0..BOARD_SIZE {
            for col in 0..BOARD_SIZE {
                let pos = Pos::new(row as u8, col as u8);
                let stone = board.get(pos);

                if stone != Stone::Empty {
                    self.draw_stone(painter, pos, stone);
                }
            }
        }
    }

    /// Draw a single stone with visual polish
    fn draw_stone(&self, painter: &Painter, pos: Pos, stone: Stone) {
        let center = self.board_to_screen(pos);
        let radius = self.cell_size * STONE_RADIUS_RATIO;

        match stone {
            Stone::Black => {
                // Shadow
                let shadow_offset = Vec2::new(2.0, 2.0);
                painter.circle_filled(
                    center + shadow_offset,
                    radius,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 60),
                );

                // Main stone
                painter.circle_filled(center, radius, BLACK_STONE);

                // Highlight
                let highlight_offset = Vec2::new(-radius * 0.3, -radius * 0.3);
                painter.circle_filled(
                    center + highlight_offset,
                    radius * 0.2,
                    BLACK_STONE_HIGHLIGHT,
                );
            }
            Stone::White => {
                // Shadow
                let shadow_offset = Vec2::new(2.0, 2.0);
                painter.circle_filled(
                    center + shadow_offset,
                    radius,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 40),
                );

                // Main stone
                painter.circle_filled(center, radius, WHITE_STONE);

                // Inner shadow for depth
                painter.circle_stroke(
                    center,
                    radius * 0.85,
                    Stroke::new(radius * 0.1, WHITE_STONE_SHADOW),
                );
            }
            Stone::Empty => {}
        }
    }

    /// Draw last move marker
    fn draw_last_move_marker(&self, painter: &Painter, pos: Pos) {
        let center = self.board_to_screen(pos);
        painter.circle_filled(center, LAST_MOVE_MARKER_RADIUS, LAST_MOVE_MARKER);
    }

    /// Draw winning line highlight
    fn draw_winning_line(&self, painter: &Painter, line: &[Pos; 5]) {
        let stroke = Stroke::new(4.0, WIN_HIGHLIGHT);

        for i in 0..4 {
            let start = self.board_to_screen(line[i]);
            let end = self.board_to_screen(line[i + 1]);
            painter.line_segment([start, end], stroke);
        }

        // Draw circles around winning stones
        for pos in line {
            let center = self.board_to_screen(*pos);
            let radius = self.cell_size * STONE_RADIUS_RATIO + 3.0;
            painter.circle_stroke(center, radius, stroke);
        }
    }

    /// Draw move suggestion
    fn draw_suggestion(&self, painter: &Painter, pos: Pos, turn: Stone) {
        let center = self.board_to_screen(pos);
        let radius = self.cell_size * STONE_RADIUS_RATIO;

        let color = match turn {
            Stone::Black => Color32::from_rgba_unmultiplied(20, 20, 20, 100),
            Stone::White => Color32::from_rgba_unmultiplied(240, 240, 240, 100),
            Stone::Empty => return,
        };

        painter.circle_filled(center, radius, color);

        // Draw "?" marker
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "?",
            egui::FontId::proportional(14.0),
            if turn == Stone::Black { WHITE_STONE } else { BLACK_STONE },
        );
    }

    /// Draw hover preview
    fn draw_hover_preview(&self, painter: &Painter, pos: Pos, turn: Stone, is_valid: bool, hover_color: Color32) {
        let center = self.board_to_screen(pos);
        let radius = self.cell_size * STONE_RADIUS_RATIO;

        let color = if is_valid {
            match turn {
                Stone::Black => Color32::from_rgba_unmultiplied(20, 20, 20, 80),
                Stone::White => Color32::from_rgba_unmultiplied(240, 240, 240, 80),
                Stone::Empty => return,
            }
        } else {
            hover_color
        };

        painter.circle_filled(center, radius, color);
    }

    /// Convert screen coordinates to board position
    pub fn screen_to_board(&self, screen_pos: Pos2) -> Option<Pos> {
        let relative = screen_pos - self.board_rect.min;
        let x = (relative.x - BOARD_MARGIN + self.cell_size * 0.5) / self.cell_size;
        let y = (relative.y - BOARD_MARGIN + self.cell_size * 0.5) / self.cell_size;

        let col = x.floor() as i32;
        let row = y.floor() as i32;

        if col >= 0 && col < BOARD_SIZE as i32 && row >= 0 && row < BOARD_SIZE as i32 {
            Some(Pos::new(row as u8, col as u8))
        } else {
            None
        }
    }

    /// Convert board position to screen coordinates
    pub fn board_to_screen(&self, pos: Pos) -> Pos2 {
        let x = self.board_rect.min.x + BOARD_MARGIN + pos.col as f32 * self.cell_size;
        let y = self.board_rect.min.y + BOARD_MARGIN + pos.row as f32 * self.cell_size;
        Pos2::new(x, y)
    }
}
