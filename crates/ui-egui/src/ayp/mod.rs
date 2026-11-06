//! AYPEditor's CapCut-style interface: Vietnamese catalog (`vi`) and, in later tasks, the panels.
pub mod vi;
pub mod tabs;
pub mod library;
pub mod side;
pub mod toolbar;
pub mod palette;

/// Show the AYP workspace (switching to it from any other) with `tab` open.
pub(crate) fn open_tab(app: &mut crate::FilmcraftApp, tab: &str) -> Result<(), String> {
    if app.ui.workspace != "AYP" {
        let w = crate::dock::find(&app.workspaces, "AYP").ok_or("no AYP workspace")?;
        app.set_workspace(&w);
    }
    app.ui.ayp_tab = tab.to_string();
    Ok(())
}
