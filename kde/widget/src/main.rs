// tasks-widget: thin Qt application shell.
//
// All task data access is performed by the QML layer via `import QtDBus`,
// calling the D-Bus service exposed by `tasks-mcp` (any serve mode).

#![deny(warnings)]

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

fn main() {
    let mut app = QGuiApplication::new();
    app.pin_mut()
        .set_application_name(&QString::from("Tasks Widget"));
    app.pin_mut()
        .set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));

    let mut engine = QQmlApplicationEngine::new();

    // Main.qml is compiled into the binary as a Qt resource by cxx-qt-build.
    let url = QUrl::from("qrc:/qt/qml/org/tasks/widget/Main.qml");
    engine.pin_mut().load(&url);

    // Run the event loop and exit with Qt's exit code.
    std::process::exit(app.pin_mut().exec());
}
