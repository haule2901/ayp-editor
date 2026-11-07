//! AYPEditor's CapCut-style interface: Vietnamese catalog (`vi`) and, in later tasks, the panels.
pub mod chat;
pub mod library;
pub mod palette;
pub mod side;
pub mod tabs;
pub mod toolbar;
pub mod vi;

/// Run a command the way an AYP button does: clear the reason an earlier click left, parse `params`,
/// invoke; a failure is said on the status line as "Chưa làm được: <lý do>".
pub(crate) fn run(app: &mut crate::FilmcraftApp, ctx: &egui::Context, cmd: &str, params: &str) -> Result<serde_json::Value, String> {
    app.ui.status.clear();
    let r = serde_json::from_str(params).map_err(|e| e.to_string()).and_then(|p| crate::menus::invoke(app, ctx, cmd, p));
    if let Err(e) = &r {
        app.ui.status = format!("Chưa làm được: {}", vi::reason(e));
    }
    r
}

/// Show the AYP workspace (switching to it from any other) with `tab` open.
pub(crate) fn open_tab(app: &mut crate::FilmcraftApp, tab: &str) -> Result<(), String> {
    if app.ui.workspace != "AYP" {
        let w = crate::dock::find(&app.workspaces, "AYP").ok_or("no AYP workspace")?;
        app.set_workspace(&w);
    }
    app.ui.ayp_tab = tab.to_string();
    Ok(())
}

/// Run `f` (a click that invokes several commands) as one undo step labelled `label`; when it fails,
/// every edit it made is taken back. `menus::invoke` needs the whole app, so it can't run inside
/// `Session::edit_group`: the group is marked around it instead.
pub(crate) fn one_step<R>(app: &mut crate::FilmcraftApp, label: &str, f: impl FnOnce(&mut crate::FilmcraftApp) -> Result<R, String>) -> Result<R, String> {
    let mark = app.session.group_begin();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(app)));
    app.session.group_end(mark, label, matches!(r, Ok(Ok(_))));
    r.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_failed_step_takes_the_whole_click_back() {
        let (mut app, ctx) = super::library::tests::demo();
        let (undo, project) = (app.session.history.undo.len(), app.session.project.clone());
        let r = super::one_step(&mut app, "Thử", |app| {
            crate::menus::invoke(app, &ctx, "sequence.addTracks", serde_json::json!({"video": 1, "audio": 0}))?;
            Err::<(), _>("then failed".to_string())
        });
        assert!(r.is_err());
        assert_eq!(app.session.history.undo.len(), undo);
        assert!(std::sync::Arc::ptr_eq(&app.session.project, &project), "the added track is taken back");
        let r = super::one_step(&mut app, "Thử", |app| {
            for _ in 0..2 {
                crate::menus::invoke(app, &ctx, "sequence.addTracks", serde_json::json!({"video": 1, "audio": 0}))?;
            }
            Ok(())
        });
        assert!(r.is_ok());
        assert_eq!(app.session.history.undo.len(), undo + 1);
        assert_eq!(app.session.history.undo.last().unwrap().0, "Thử");
    }
}
