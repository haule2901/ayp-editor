//! `FilmcraftApp::toast` is drawn (the busy control port notice relies on it) and clears itself.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;

#[test]
fn toast_is_drawn_and_expires() {
    let mut app = FilmcraftApp::new(Session::default());
    app.toast = Some(("Cổng điều khiển 9879 đang bận — AI/MCP chưa nối được.".into(), 0.0));
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
    for _ in 0..4 {
        h.step();
    }
    assert!(h.query_by_label_contains("đang bận").is_some(), "toast text not drawn");
    h.state_mut().toast = Some(("old".into(), -100.0));
    for _ in 0..2 {
        h.step();
    }
    assert!(h.state().toast.is_none(), "expired toast must clear");
}
