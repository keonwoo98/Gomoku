//! Theme constants for the Gomoku GUI

use egui::Color32;

// Board colors
pub const BOARD_BG: Color32 = Color32::from_rgb(220, 179, 92);
pub const GRID_LINE: Color32 = Color32::from_rgb(40, 30, 20);
pub const STAR_POINT: Color32 = Color32::from_rgb(40, 30, 20);

// Stone colors
pub const BLACK_STONE: Color32 = Color32::from_rgb(20, 20, 20);
pub const BLACK_STONE_HIGHLIGHT: Color32 = Color32::from_rgb(60, 60, 60);
pub const WHITE_STONE: Color32 = Color32::from_rgb(245, 245, 245);
pub const WHITE_STONE_SHADOW: Color32 = Color32::from_rgb(180, 180, 180);

// Markers
pub const LAST_MOVE_MARKER: Color32 = Color32::from_rgb(220, 50, 50);
pub const WIN_HIGHLIGHT: Color32 = Color32::from_rgb(50, 205, 50);

// Functions for colors that can't be const
pub fn hover_valid() -> Color32 {
    Color32::from_rgba_unmultiplied(100, 100, 100, 120)
}

pub fn hover_invalid() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 0, 0, 80)
}

// UI colors
pub const PANEL_BG: Color32 = Color32::from_rgb(45, 45, 48);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(230, 230, 230);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 160, 160);
pub const BUTTON_BG: Color32 = Color32::from_rgb(70, 70, 75);
pub const BUTTON_HOVER: Color32 = Color32::from_rgb(90, 90, 95);

// Timer colors
pub const TIMER_NORMAL: Color32 = Color32::from_rgb(100, 200, 100);
pub const TIMER_WARNING: Color32 = Color32::from_rgb(255, 200, 50);
pub const TIMER_CRITICAL: Color32 = Color32::from_rgb(255, 80, 80);

// Sizes
pub const BOARD_MARGIN: f32 = 40.0;
pub const STONE_RADIUS_RATIO: f32 = 0.45;
pub const STAR_POINT_RADIUS: f32 = 4.0;
pub const GRID_LINE_WIDTH: f32 = 1.0;
pub const LAST_MOVE_MARKER_RADIUS: f32 = 5.0;

// Star point positions (0-indexed)
pub const STAR_POINTS: [(u8, u8); 9] = [
    (3, 3), (3, 9), (3, 15),
    (9, 3), (9, 9), (9, 15),
    (15, 3), (15, 9), (15, 15),
];
