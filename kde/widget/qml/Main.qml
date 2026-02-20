// Main.qml — Tasks Widget
//
// Displays open tasks from the tasks-mcp D-Bus service.
// Supports:
//   • Filter by project (task list) or global (all lists)
//   • Group by project or priority
//
// D-Bus service: org.tasks.TasksMcp at /org/tasks/TasksMcp
// D-Bus activation is automatic — the service is started on demand.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Layouts 1.15
import QtDBus 2.15

ApplicationWindow {
    id: root
    visible: true
    width: 480
    height: 680
    title: "Tasks"

    Material.theme: Material.System

    // ---- D-Bus interface ---------------------------------------------------
    DBusInterface {
        id: tasksDbus
        service:   "org.tasks.TasksMcp"
        path:      "/org/tasks/TasksMcp"
        iface:     "org.tasks.TasksMcp"
        bus:       DBusConnection.SessionBus
    }

    // ---- Helper: call a D-Bus method synchronously -------------------------
    function dbusCall(method, args) {
        try {
            return tasksDbus.call(method, args)
        } catch (e) {
            return JSON.stringify({ error: e.toString() })
        }
    }

    // ---- State -------------------------------------------------------------
    property var allTasks:    []
    property var lists:       []
    property string searchText: ""
    property bool loading:    false
    property string errorMsg: ""

    // ---- Load tasks --------------------------------------------------------
    function reload() {
        loading  = true
        errorMsg = ""

        // Fetch available lists
        var listsRaw = dbusCall("ListLists", [])
        try {
            var lp = JSON.parse(listsRaw)
            lists = Array.isArray(lp) ? lp : []
            if (lp && lp.error) errorMsg = "D-Bus: " + lp.error
        } catch (e) { errorMsg = "Parse: " + e }

        // Fetch tasks — apply project filter at the server when set
        var filterInput = filterCombo.currentIndex > 0
            ? JSON.stringify({ list: filterCombo.currentText })
            : "{}"
        var tasksRaw = dbusCall("ListTasks", [filterInput])
        try {
            var tp = JSON.parse(tasksRaw)
            if (Array.isArray(tp)) {
                allTasks = tp
            } else {
                if (tp && tp.error) errorMsg = "D-Bus: " + tp.error
                allTasks = []
            }
        } catch (e) {
            errorMsg  = "Parse: " + e
            allTasks  = []
        }

        loading = false
        taskListModel.rebuild()
    }

    // ---- Client-side filter + sort for grouping ----------------------------
    function applyFilters(raw) {
        var result = []
        var needle = searchText.toLowerCase()
        for (var i = 0; i < raw.length; i++) {
            var t = raw[i]
            if (!todoFilter.checked    && t.status === "todo")    continue
            if (!doingFilter.checked   && t.status === "doing")   continue
            if (!blockedFilter.checked && t.status === "blocked") continue
            if (needle !== "" && t.title.toLowerCase().indexOf(needle) < 0) continue
            result.push(t)
        }
        var gb = groupCombo.currentIndex
        result.sort(function(a, b) {
            if (gb === 1) {
                var lc = (a.list || "").localeCompare(b.list || "")
                if (lc !== 0) return lc
            } else if (gb === 2) {
                var pa = priorityOrder(a.priority)
                var pb = priorityOrder(b.priority)
                if (pa !== pb) return pa - pb
            }
            return (a.title || "").localeCompare(b.title || "")
        })
        return result
    }

    function priorityOrder(p) {
        switch(p) {
            case "p0": return 0; case "p1": return 1
            case "p2": return 2; case "p3": return 3
            default:   return 99
        }
    }

    function sectionLabel(task) {
        var gb = groupCombo.currentIndex
        if (gb === 1) return task.list     || "—"
        if (gb === 2) return task.priority ? task.priority.toUpperCase() : "No priority"
        return ""
    }

    // ---- ListModel rebuilt whenever filters change -------------------------
    ListModel {
        id: taskListModel

        function rebuild() {
            clear()
            var filtered = applyFilters(allTasks)
            var prevSection = null
            var gb = groupCombo.currentIndex
            for (var i = 0; i < filtered.length; i++) {
                var t = filtered[i]
                var sec = sectionLabel(t)
                if (gb > 0 && sec !== prevSection) {
                    append({ isSectionHeader: true,  sectionTitle: sec,
                              id: "", title: "", status: "", priority: "",
                              list: "", updated: "", task_type: "", due: "" })
                    prevSection = sec
                }
                append({
                    isSectionHeader: false, sectionTitle: "",
                    id:        t.id        || "",
                    title:     t.title     || "",
                    status:    t.status    || "",
                    priority:  t.priority  || "",
                    list:      t.list      || "",
                    updated:   t.updated   || "",
                    task_type: t.type      || "",
                    due:       t.due       || ""
                })
            }
        }
    }

    // ---- Toolbar -----------------------------------------------------------
    header: ToolBar {
        ColumnLayout {
            anchors { left: parent.left; right: parent.right; margins: 8 }
            spacing: 4

            RowLayout {
                Layout.fillWidth: true
                Label { text: "Tasks"; font.bold: true; font.pixelSize: 18; Layout.fillWidth: true }
                ToolButton {
                    text: "↻"
                    onClicked: reload()
                    enabled: !loading
                    ToolTip { text: "Refresh"; visible: parent.hovered }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Label { text: "Project:" }
                ComboBox {
                    id: filterCombo
                    Layout.fillWidth: true
                    model: ["All"].concat(lists)
                    onCurrentIndexChanged: if (root.allTasks.length > 0) taskListModel.rebuild()
                }
                Label { text: "Group:" }
                ComboBox {
                    id: groupCombo
                    model: ["None", "Project", "Priority"]
                    onCurrentIndexChanged: taskListModel.rebuild()
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 4
                TextField {
                    id: searchField
                    Layout.fillWidth: true
                    placeholderText: "Search…"
                    onTextChanged: { searchText = text; taskListModel.rebuild() }
                }
                Button { id: todoFilter;    text: "Todo";    checkable: true; checked: true; highlighted: checked; onCheckedChanged: taskListModel.rebuild() }
                Button { id: doingFilter;   text: "Doing";   checkable: true; checked: true; highlighted: checked; onCheckedChanged: taskListModel.rebuild() }
                Button { id: blockedFilter; text: "Blocked"; checkable: true; checked: true; highlighted: checked; onCheckedChanged: taskListModel.rebuild() }
            }
        }
    }

    // ---- Body --------------------------------------------------------------
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            visible: errorMsg !== ""
            Layout.fillWidth: true
            height: visible ? errLabel.implicitHeight + 16 : 0
            color: Material.color(Material.Red, Material.Shade100)
            Label {
                id: errLabel
                anchors { fill: parent; margins: 8 }
                text: errorMsg
                color: Material.color(Material.Red, Material.Shade900)
                wrapMode: Text.WordWrap
            }
        }

        BusyIndicator { Layout.alignment: Qt.AlignHCenter; running: loading; visible: loading }

        ListView {
            id: listView
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: taskListModel
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Loader {
                width: listView.width
                sourceComponent: model.isSectionHeader ? sectionHeaderComponent
                                                       : taskDelegateComponent
                property var itemModel: model
            }
        }

        Label {
            visible: !loading && taskListModel.count === 0 && errorMsg === ""
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: "No tasks found."
            opacity: 0.5
            padding: 32
        }
    }

    // ---- Section header component ------------------------------------------
    Component {
        id: sectionHeaderComponent
        Rectangle {
            height: 28
            color: Material.color(Material.Grey, Material.Shade200)
            Label {
                anchors { left: parent.left; right: parent.right
                          verticalCenter: parent.verticalCenter; leftMargin: 12 }
                text: itemModel.sectionTitle
                font.bold: true; font.pixelSize: 12
                color: Material.color(Material.Grey, Material.Shade700)
            }
        }
    }

    // ---- Task delegate component -------------------------------------------
    Component {
        id: taskDelegateComponent
        TaskDelegate {
            width: listView.width
            taskId:       itemModel.id
            taskTitle:    itemModel.title
            taskStatus:   itemModel.status
            taskPriority: itemModel.priority
            taskList:     itemModel.list
            taskDue:      itemModel.due
            taskType:     itemModel.task_type

            onStatusChangeRequested: function(newStatus) {
                var input = JSON.stringify({ id: taskId, status: newStatus })
                tasksDbus.call("SetStatus", [input])
                reload()
            }
        }
    }

    Component.onCompleted: reload()
}
