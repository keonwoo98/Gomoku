//! Theme constants for the Gomoku GUI

use egui::Color32;

// Board colors - warm wood tones
pub const BOARD_BG: Color32 = Color32::from_rgb(222, 184, 135); // Burlywood
#[allow(dead_code)]
pub const BOARD_BORDER: Color32 = Color32::from_rgb(139, 90, 43);  // Saddle brown
pub const GRID_LINE: Color32 = Color32::from_rgb(60, 40, 20);
pub const STAR_POINT: Color32 = Color32::from_rgb(50, 35, 20);

// Stone colors with better contrast
pub const BLACK_STONE: Color32 = Color32::from_rgb(25, 25, 30);
pub const BLACK_STONE_HIGHLIGHT: Color32 = Color32::from_rgb(70, 70, 80);
pub const WHITE_STONE: Color32 = Color32::from_rgb(250, 250, 252);
pub const WHITE_STONE_SHADOW: Color32 = Color32::from_rgb(190, 190, 195);

// Markers
pub const LAST_MOVE_MARKER: Color32 = Color32::from_rgb(230, 60, 60);
pub const WIN_HIGHLIGHT: Color32 = Color32::from_rgb(50, 220, 50);

// Capture effect colors (used in board_view animation)
#[allow(dead_code)]
pub const CAPTURE_FLASH: Color32 = Color32::from_rgb(255, 100, 100);
#[allow(dead_code)]
pub const CAPTURE_RING: Color32 = Color32::from_rgb(255, 50, 50);

// Functions for colors that can't be const
pub fn hover_valid() -> Color32 {
    Color32::from_rgba_unmultiplied(80, 80, 80, 100)
}

pub fn hover_invalid() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 50, 50, 100)
}

// Panel colors - dark modern theme
#[allow(dead_code)]
pub const PANEL_BG: Color32 = Color32::from_rgb(32, 34, 37);
#[allow(dead_code)]
pub const PANEL_HEADER: Color32 = Color32::from_rgb(42, 44, 48);
#[allow(dead_code)]
pub const PANEL_BORDER: Color32 = Color32::from_rgb(60, 62, 66);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 240, 245);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 165, 175);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(120, 125, 135);

// Button colors
#[allow(dead_code)]
pub const BUTTON_BG: Color32 = Color32::from_rgb(55, 57, 62);
#[allow(dead_code)]
pub const BUTTON_HOVER: Color32 = Color32::from_rgb(70, 72, 78);
#[allow(dead_code)]
pub const BUTTON_ACTIVE: Color32 = Color32::from_rgb(85, 87, 95);

// Status colors
#[allow(dead_code)]
pub const STATUS_BLACK: Color32 = Color32::from_rgb(60, 60, 65);
#[allow(dead_code)]
pub const STATUS_WHITE: Color32 = Color32::from_rgb(220, 220, 225);

// Timer colors
pub const TIMER_NORMAL: Color32 = Color32::from_rgb(80, 200, 120);
pub const TIMER_WARNING: Color32 = Color32::from_rgb(255, 180, 50);
pub const TIMER_CRITICAL: Color32 = Color32::from_rgb(255, 70, 70);

// Score/capture colors
#[allow(dead_code)]
pub const CAPTURE_BLACK_BG: Color32 = Color32::from_rgb(50, 50, 55);
#[allow(dead_code)]
pub const CAPTURE_WHITE_BG: Color32 = Color32::from_rgb(200, 200, 205);

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
