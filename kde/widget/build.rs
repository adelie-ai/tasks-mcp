use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    // No custom bridge types — the widget is pure QML + cxx_qt_lib app shell.
    // QML files are embedded as Qt resources.
    let module = QmlModule::new("org.tasks.widget")
        .version(1, 0)
        .qml_file("qml/Main.qml")
        .qml_file("qml/TaskDelegate.qml");

    CxxQtBuilder::new_qml_module(module)
        .qt_module("Gui")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("DBus")
        .build();
}
