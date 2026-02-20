// TaskDelegate.qml — A single task card in the Tasks plasmoid
//
// Exposed properties mirror the fields of a TaskSummary JSON object.
// Signal: statusChangeRequested(newStatus)

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import org.kde.kirigami as Kirigami

ItemDelegate {
    id: root

    // ---- Public API --------------------------------------------------------
    property string taskId:       ""
    property string taskTitle:    ""
    property string taskStatus:   ""
    property string taskPriority: ""
    property string taskList:     ""
    property string taskDue:      ""
    property string taskType:     ""

    signal statusChangeRequested(string newStatus)
    signal appendNoteRequested(string note)
    signal priorityChangeRequested(string newPriority)

    property bool noteInputVisible: false

    // ---- Right-click context menu ------------------------------------------
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        propagateComposedEvents: true
        onClicked: function(mouse) {
            if (mouse.button === Qt.RightButton) {
                statusMenu.popup()
            }
        }
    }

    Menu {
        id: statusMenu
        MenuItem { text: "todo";        onTriggered: root.statusChangeRequested("todo")       }
        MenuItem { text: "doing";       onTriggered: root.statusChangeRequested("doing")      }
        MenuItem { text: "blocked";     onTriggered: root.statusChangeRequested("blocked")    }
        MenuItem { text: "validating";  onTriggered: root.statusChangeRequested("validating") }
        MenuItem { text: "done";        onTriggered: root.statusChangeRequested("done")       }
        MenuItem { text: "canceled";    onTriggered: root.statusChangeRequested("canceled")   }
        MenuSeparator {}
        Menu {
            title: "Set priority"
            MenuItem { text: "P0 — critical";  onTriggered: root.priorityChangeRequested("p0") }
            MenuItem { text: "P1 — high";      onTriggered: root.priorityChangeRequested("p1") }
            MenuItem { text: "P2 — medium";    onTriggered: root.priorityChangeRequested("p2") }
            MenuItem { text: "P3 — low";       onTriggered: root.priorityChangeRequested("p3") }
            MenuSeparator {}
            MenuItem { text: "None";           onTriggered: root.priorityChangeRequested("none") }
        }
        MenuSeparator {}
        MenuItem { text: "Add note…";   onTriggered: { root.noteInputVisible = true; noteField.forceActiveFocus() } }
    }

    // ---- Appearance --------------------------------------------------------
    height: contentLayout.implicitHeight + Kirigami.Units.largeSpacing
    hoverEnabled: true
    background: Rectangle {
        color: root.hovered ? Kirigami.Theme.hoverColor : "transparent"
        Rectangle {
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            width: Kirigami.Units.smallSpacing / 2
            color: priorityColor(taskPriority)
            radius: 1
        }
    }

    // ---- Content -----------------------------------------------------------
    contentItem: ColumnLayout {
        id: contentLayout
        spacing: 0

        RowLayout {
            id: mainRow
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.smallSpacing

            // Status indicator circle
            Rectangle {
                width: Kirigami.Units.iconSizes.small * 0.6
                height: width
                radius: width / 2
                color: statusColor(taskStatus)
                border { color: Qt.darker(color, 1.3); width: 1 }
                Layout.alignment: Qt.AlignVCenter

                ToolTip.text: taskStatus
                ToolTip.visible: statusHover.containsMouse
                MouseArea { id: statusHover; anchors.fill: parent; hoverEnabled: true }
            }

            // Text block
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                Label {
                    Layout.fillWidth: true
                    text: taskTitle
                    font.pixelSize: Kirigami.Units.gridUnit * 0.85
                    elide: Text.ElideRight
                    color: Kirigami.Theme.textColor
                    opacity: (taskStatus === "done" || taskStatus === "canceled") ? 0.5 : 1.0
                    font.strikeout: taskStatus === "done" || taskStatus === "canceled"
                }

                // Meta row: project • due • type
                RowLayout {
                    spacing: Kirigami.Units.smallSpacing
                    visible: taskList !== "" || taskDue !== "" || taskType !== ""

                    Label {
                        visible: taskList !== ""
                        text: taskList
                        font.pixelSize: Kirigami.Units.gridUnit * 0.65
                        color: Kirigami.Theme.linkColor
                        font.italic: true
                    }
                    Label {
                        visible: taskList !== "" && taskDue !== ""
                        text: "·"
                        font.pixelSize: Kirigami.Units.gridUnit * 0.65
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Label {
                        visible: taskDue !== ""
                        text: "Due " + taskDue.substring(0, 10)
                        font.pixelSize: Kirigami.Units.gridUnit * 0.65
                        color: dueDateColor(taskDue)
                    }
                    Label {
                        visible: taskType !== ""
                        text: "[" + taskType + "]"
                        font.pixelSize: Kirigami.Units.gridUnit * 0.65
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }

            // Priority badge
            Rectangle {
                visible: taskPriority !== ""
                width: prioLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
                height: Kirigami.Units.gridUnit
                radius: Kirigami.Units.cornerRadius
                color: priorityColor(taskPriority)
                opacity: 0.85

                Label {
                    id: prioLabel
                    anchors.centerIn: parent
                    text: taskPriority.toUpperCase()
                    font.pixelSize: Kirigami.Units.gridUnit * 0.6
                    font.bold: true
                    color: "white"
                }
            }

            // Status-change button
            ToolButton {
                text: "⋮"
                font.pixelSize: Kirigami.Units.gridUnit
                Layout.alignment: Qt.AlignVCenter
                onClicked: statusMenu.popup()
            }
        } // mainRow

        // Inline note input (shown when "Add note…" is triggered)
        RowLayout {
            id: noteInputRow
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.smallSpacing
            Layout.bottomMargin: Kirigami.Units.smallSpacing / 2
            visible: root.noteInputVisible
            spacing: Kirigami.Units.smallSpacing

            TextField {
                id: noteField
                Layout.fillWidth: true
                placeholderText: "Quick note…"
                font.pixelSize: Kirigami.Units.gridUnit * 0.8
                Keys.onReturnPressed: root.submitNote()
                Keys.onEscapePressed: { root.noteInputVisible = false; noteField.text = "" }
            }

            ToolButton {
                text: "✓"
                enabled: noteField.text.trim() !== ""
                ToolTip.text: "Append note"
                ToolTip.visible: hovered
                onClicked: root.submitNote()
            }

            ToolButton {
                text: "✕"
                ToolTip.text: "Cancel"
                ToolTip.visible: hovered
                onClicked: { root.noteInputVisible = false; noteField.text = "" }
            }
        }
    } // contentLayout

    // ---- Helpers -----------------------------------------------------------
    function submitNote() {
        var t = noteField.text.trim()
        if (t === "") { return }
        root.appendNoteRequested(t)
        noteField.text = ""
        root.noteInputVisible = false
    }

    function statusColor(s) {
        switch (s) {
            case "todo":       return "#95a5a6"
            case "doing":      return "#2980b9"
            case "blocked":    return "#e67e22"
            case "validating": return "#8e44ad"
            case "done":       return "#27ae60"
            case "canceled":   return "#e74c3c"
            default:           return "#bdc3c7"
        }
    }

    function priorityColor(p) {
        switch (p) {
            case "p0": return "#c0392b"
            case "p1": return "#e67e22"
            case "p2": return "#2980b9"
            case "p3": return "#7f8c8d"
            default:   return "#95a5a6"
        }
    }

    function dueDateColor(due) {
        if (!due) return Kirigami.Theme.disabledTextColor
        var d    = new Date(due)
        var now  = new Date()
        var diff = (d - now) / (1000 * 60 * 60 * 24)
        if (diff < 0)  return Kirigami.Theme.negativeTextColor
        if (diff <= 3) return "#e67e22"
        return Kirigami.Theme.disabledTextColor
    }
}
