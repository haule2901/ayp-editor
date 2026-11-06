//! Placeholder; the real panel arrives in a later task.
use crate::FilmcraftApp;

pub fn show(_app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: egui::Rect) {
    ui.put(rect, egui::Label::new("Bảng bên"));
}
