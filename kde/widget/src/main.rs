// tasks-widget: thin Qt application shell.
//
// All task data access is performed by the QML layer via `import QtDBus`,
// calling the D-Bus service exposed by `tasks-mcp` (any serve mode).

#![deny(warnings)]

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use std::path::PathBuf;

/// Candidate directories to search for the QML files, in priority order.
fn qml_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // 1. Next to the running binary (works when running from build dir or
    //    when QML files are copied alongside the installed binary).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("qml/Main.qml"));
        }
    }

    // 2. XDG data home (default: ~/.local/share/tasks-mcp/qml/Main.qml).
    let xdg_data = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
            PathBuf::from(home).join(".local/share")
        });
    candidates.push(xdg_data.join("tasks-mcp/qml/Main.qml"));

    // 3. Source tree location (convenient when running via `cargo run`).
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest).join("qml/Main.qml"));
    }

    candidates
}

fn main() {
    let mut app = QGuiApplication::new();
    app.pin_mut()
        .set_application_name(&QString::from("Tasks Widget"));
    app.pin_mut()
        .set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));

    let qml_path = qml_candidates()
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| {
            eprintln!("tasks-widget: could not find Main.qml in any search path.");
            eprintln!("  Install QML files to ~/.local/share/tasks-mcp/qml/ or run 'just widget-install'");
            std::process::exit(1);
        });

    let url = QUrl::from(format!("file://{}", qml_path.display()).as_str());
    let mut engine = QQmlApplicationEngine::new();
    engine.pin_mut().load(&url);

    // Run the event loop and exit with Qt's exit code.
    std::process::exit(app.pin_mut().exec());
}
