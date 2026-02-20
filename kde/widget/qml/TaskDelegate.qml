// TaskDelegate.qml — A single task card in the Tasks Widget
//
// Exposed properties mirror the fields of a TaskSummary JSON object.
// Signal: statusChangeRequested(newStatus) — emitted when the user picks a
//         new status from the inline context-menu button.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Layouts 1.15

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

    // ---- Appearance --------------------------------------------------------
    height: contentLayout.implicitHeight + 16
    hoverEnabled: true
    background: Rectangle {
        color: root.hovered
               ? Material.color(Material.Grey, Material.Shade50)
               : "transparent"
        Rectangle {
            // Left accent strip — coloured by priority
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            width: 4
            color: priorityColor(taskPriority)
            radius: 2
        }
    }

    // ---- Content -----------------------------------------------------------
    contentItem: RowLayout {
        id: contentLayout
        spacing: 10
        anchors { left: parent.left; right: parent.right;
                  leftMargin: 16; rightMargin: 8; verticalCenter: parent.verticalCenter }

        // Status indicator circle
        Rectangle {
            width: 14; height: 14
            radius: 7
            color: statusColor(taskStatus)
            border { color: Qt.darker(color, 1.3); width: 1 }
            anchors.verticalCenter: parent.verticalCenter

            ToolTip.text: taskStatus
            ToolTip.visible: statusMouseArea.containsMouse
            MouseArea {
                id: statusMouseArea
                anchors.fill: parent
                hoverEnabled: true
            }
        }

        // Text block
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            // Title
            Label {
                Layout.fillWidth: true
                text: taskTitle
                font.pixelSize: 14
                elide: Text.ElideRight
                color: taskStatus === "done" || taskStatus === "canceled"
                       ? Material.foreground
                       : Material.primaryTextColor
                font.strikeout: taskStatus === "done" || taskStatus === "canceled"
            }

            // Meta row: project • due • type
            RowLayout {
                spacing: 6
                visible: taskList !== "" || taskDue !== "" || taskType !== ""

                Label {
                    visible: taskList !== ""
                    text: taskList
                    font.pixelSize: 11
                    color: Material.color(Material.Blue, Material.Shade600)
                    font.italic: true
                }
                Label {
                    visible: taskList !== "" && taskDue !== ""
                    text: "•"
                    font.pixelSize: 11
                    color: Material.hintTextColor
                }
                Label {
                    visible: taskDue !== ""
                    text: "Due " + taskDue.substring(0, 10)
                    font.pixelSize: 11
                    color: dueDateColor(taskDue)
                }
                Label {
                    visible: taskType !== ""
                    text: "[" + taskType + "]"
                    font.pixelSize: 11
                    color: Material.hintTextColor
                }
            }
        }

        // Priority badge
        Rectangle {
            visible: taskPriority !== ""
            width: priorityLabel.implicitWidth + 8
            height: 18
            radius: 4
            color: priorityColor(taskPriority)
            opacity: 0.85

            Label {
                id: priorityLabel
                anchors.centerIn: parent
                text: taskPriority.toUpperCase()
                font.pixelSize: 10
                font.bold: true
                color: "white"
            }
        }

        // Status-change button
        ToolButton {
            text: "⋮"
            font.pixelSize: 18
            anchors.verticalCenter: parent.verticalCenter
            onClicked: statusMenu.open()

            Menu {
                id: statusMenu
                title: "Set status"
                MenuItem { text: "todo";     onTriggered: root.statusChangeRequested("todo")     }
                MenuItem { text: "doing";    onTriggered: root.statusChangeRequested("doing")    }
                MenuItem { text: "blocked";  onTriggered: root.statusChangeRequested("blocked")  }
                MenuItem { text: "done";     onTriggered: root.statusChangeRequested("done")     }
                MenuItem { text: "canceled"; onTriggered: root.statusChangeRequested("canceled") }
            }
        }
    }

    // ---- Helper functions --------------------------------------------------
    function statusColor(s) {
        switch (s) {
            case "todo":     return Material.color(Material.Grey,   Material.Shade400)
            case "doing":    return Material.color(Material.Blue,   Material.Shade500)
            case "blocked":  return Material.color(Material.Orange, Material.Shade600)
            case "done":     return Material.color(Material.Green,  Material.Shade500)
            case "canceled": return Material.color(Material.Red,    Material.Shade300)
            default:         return Material.color(Material.Grey,   Material.Shade300)
        }
    }

    function priorityColor(p) {
        switch (p) {
            case "p0": return Material.color(Material.Red,    Material.Shade700)
            case "p1": return Material.color(Material.Orange, Material.Shade600)
            case "p2": return Material.color(Material.Blue,   Material.Shade500)
            case "p3": return Material.color(Material.Grey,   Material.Shade500)
            default:   return Material.color(Material.Grey,   Material.Shade400)
        }
    }

    function dueDateColor(due) {
        if (!due) return Material.hintTextColor
        var d = new Date(due)
        var now = new Date()
        var diff = (d - now) / (1000 * 60 * 60 * 24) // days
        if (diff < 0)  return Material.color(Material.Red,    Material.Shade600)
        if (diff <= 3) return Material.color(Material.Orange, Material.Shade700)
        return Material.hintTextColor
    }
}
