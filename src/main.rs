//! Gomoku AI Engine GUI
//!
//! A graphical interface for playing Gomoku with AI or against another player.

use gomoku::ui::GomokuApp;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 750.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Gomoku - 42 Project"),
        ..Default::default()
    };

    eframe::run_native(
        "Gomoku",
        options,
        Box::new(|cc| Ok(Box::new(GomokuApp::new(cc)))),
    )
}
