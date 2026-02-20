// main.qml — Tasks plasmoid
//
// Displays open tasks from the tasks-mcp D-Bus service.
// D-Bus is accessed via a Python helper (contents/code/tasks_client.py)
// which calls gdbus — the QtDBus QML module was removed in Qt 6.
//
// Run standalone: plasmawindowed org.tasks.widget

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasmoid
import org.kde.plasma.plasma5support as Plasma5Support

PlasmoidItem {
    id: root

    preferredRepresentation: fullRepresentation
    Plasmoid.title: "Tasks"

    // ---- Compact representation (panel icon) --------------------------------
    compactRepresentation: Item {
        Kirigami.Icon {
            anchors.centerIn: parent
            source: "view-task"
            width: Math.min(parent.width, parent.height)
            height: width
        }
        MouseArea {
            anchors.fill: parent
            onClicked: root.expanded = !root.expanded
        }
    }

    // ---- Full representation -----------------------------------------------
    fullRepresentation: Item {
        id: full
        implicitWidth:  Kirigami.Units.gridUnit * 28
        implicitHeight: Kirigami.Units.gridUnit * 40

        Layout.minimumWidth:  Kirigami.Units.gridUnit * 22
        Layout.minimumHeight: Kirigami.Units.gridUnit * 28
        Layout.preferredWidth:  implicitWidth
        Layout.preferredHeight: implicitHeight

        // Path to the Python D-Bus helper bundled inside this plasmoid package.
        readonly property string helperPath: Qt.resolvedUrl("../code/tasks_client.py")
            .toString().replace("file://", "")

        // ---- Subprocess runner (Plasma5Support executable engine) -----------
        Plasma5Support.DataSource {
            id: executable
            engine: "executable"
            connectedSources: []

            onNewData: function(sourceName, data) {
                disconnectSource(sourceName)
                var idx = full._pendingCmds.indexOf(sourceName)
                if (idx < 0) { return }

                var onSuccess = full._pendingSuccess[idx]
                var onError   = full._pendingError[idx]
                var onAny     = full._pendingOnAny[idx]
                full._pendingCmds.splice(idx, 1)
                full._pendingSuccess.splice(idx, 1)
                full._pendingError.splice(idx, 1)
                full._pendingOnAny.splice(idx, 1)

                var stdout   = (data["stdout"]    || "").trim()
                var stderr   = (data["stderr"]    || "").trim()
                var exitCode =  data["exit code"] || 0

                if (onAny) {
                    onAny(exitCode, stdout, stderr)
                } else if (exitCode !== 0) {
                    if (onError) { onError(stderr || ("exit code " + exitCode)) }
                } else {
                    if (onSuccess) { onSuccess(stdout) }
                }
            }
        }

        property var _pendingCmds:    []
        property var _pendingSuccess: []
        property var _pendingError:   []
        property var _pendingOnAny:   []

        function runCommand(cmd, onSuccess, onError, onAny) {
            // Append a unique nonce so identical commands can queue concurrently.
            var unique = cmd + " #" + Date.now()
            _pendingCmds.push(unique)
            _pendingSuccess.push(onSuccess || null)
            _pendingError.push(onError   || null)
            _pendingOnAny.push(onAny     || null)
            executable.connectSource(unique)
        }

        function helper(args) {
            return "python3 " + helperPath + " " + args
        }

        // ---- State ---------------------------------------------------------
        property var    allTasks:   []
        property var    lists:      []
        property string searchText: ""
        property bool   loading:    false
        property string errorMsg:   ""

        // ---- D-Bus signal watcher -----------------------------------------
        // Runs tasks_client.py watch-signal in a persistent loop.
        // Exit 0 = TasksChanged fired  → reload tasks + restart watcher.
        // Exit 2 = timeout (60 s)      → restart watcher (no reload).
        // Exit other                   → log error, restart after short delay.
        function watchSignal() {
            runCommand(
                helper("watch-signal --timeout 60"),
                null, null,
                function(exitCode, _out, err) {
                    if (exitCode === 0) {
                        // TasksChanged signal received — refresh and keep watching.
                        reload()
                    } else if (exitCode !== 2) {
                        // Unexpected error (gi unavailable, bus gone, etc.).
                        console.warn("tasks-widget: signal watcher error:", err)
                    }
                    // Always restart: exit 0 and 2 immediately, errors after
                    // a short back-off so we don't spin on a broken bus.
                    if (exitCode === 0 || exitCode === 2) {
                        watchSignal()
                    } else {
                        Qt.callLater(watchSignal)
                    }
                }
            )
        }

        // ---- Load tasks (list-lists then list-tasks) -----------------------
        function reload() {
            if (loading) { return }
            loading  = true
            errorMsg = ""

            runCommand(
                helper("list-lists"),
                function(stdout) {
                    try {
                        var lp = JSON.parse(stdout)
                        lists = Array.isArray(lp) ? lp : []
                        if (lp && lp.error) { errorMsg = lp.error }
                    } catch (e) { errorMsg = "Parse error: " + e }
                    fetchTasks()
                },
                function(err) {
                    errorMsg = "list-lists: " + err
                    lists = []
                    fetchTasks()
                }
            )
        }

        function fetchTasks() {
            var filterArg = (filterCombo.currentIndex > 0)
                ? "list-tasks '" + JSON.stringify({list: filterCombo.currentText}).replace(/'/g, "'\\''" ) + "'"
                : "list-tasks"
            runCommand(
                helper(filterArg),
                function(stdout) {
                    try {
                        var tp = JSON.parse(stdout)
                        if (Array.isArray(tp)) {
                            allTasks = tp
                        } else {
                            if (tp && tp.error) { errorMsg = tp.error }
                            allTasks = []
                        }
                    } catch (e) {
                        errorMsg = "Parse error: " + e
                        allTasks = []
                    }
                    loading = false
                    taskListModel.rebuild()
                },
                function(err) {
                    errorMsg = "list-tasks: " + err
                    allTasks = []
                    loading  = false
                    taskListModel.rebuild()
                }
            )
        }

        // ---- Client-side filter + sort -------------------------------------
        function applyFilters(raw) {
            var result = []
            var needle = searchText.toLowerCase()
            for (var i = 0; i < raw.length; i++) {
                var t = raw[i]
                if (!todoFilter.checked        && t.status === "todo")        { continue }
                if (!doingFilter.checked       && t.status === "doing")       { continue }
                if (!blockedFilter.checked     && t.status === "blocked")     { continue }
                if (!validatingFilter.checked  && t.status === "validating")  { continue }
                if (!doneFilter.checked        && (t.status === "done" || t.status === "canceled")) { continue }
                if (needle !== "" && t.title.toLowerCase().indexOf(needle) < 0) { continue }
                result.push(t)
            }
            var gb = groupCombo.currentIndex
            result.sort(function(a, b) {
                if (gb === 1) {
                    var lc = (a.list || "").localeCompare(b.list || "")
                    if (lc !== 0) { return lc }
                } else if (gb === 2) {
                    var pa = priorityOrder(a.priority)
                    var pb = priorityOrder(b.priority)
                    if (pa !== pb) { return pa - pb }
                }
                return (a.title || "").localeCompare(b.title || "")
            })
            return result
        }

        function priorityOrder(p) {
            switch (p) {
                case "p0": return 0
                case "p1": return 1
                case "p2": return 2
                case "p3": return 3
                default:   return 99
            }
        }

        function sectionLabel(task) {
            var gb = groupCombo.currentIndex
            if (gb === 1) { return task.list     || "—" }
            if (gb === 2) { return task.priority ? task.priority.toUpperCase() : "No priority" }
            return ""
        }

        // ---- ListModel -----------------------------------------------------
        ListModel {
            id: taskListModel

            function rebuild() {
                clear()
                var filtered = full.applyFilters(full.allTasks)
                var prevSection = null
                var gb = groupCombo.currentIndex
                for (var i = 0; i < filtered.length; i++) {
                    var t = filtered[i]
                    var sec = full.sectionLabel(t)
                    if (gb > 0 && sec !== prevSection) {
                        append({ isSectionHeader: true,  sectionTitle: sec,
                                 id: "", title: "", status: "", priority: "",
                                 list: "", task_type: "", due: "" })
                        prevSection = sec
                    }
                    append({
                        isSectionHeader: false, sectionTitle: "",
                        id:        t.id        || "",
                        title:     t.title     || "",
                        status:    t.status    || "",
                        priority:  t.priority  || "",
                        list:      t.list      || "",
                        task_type: t.type      || "",
                        due:       t.due       || ""
                    })
                }
            }
        }

        // ---- Layout --------------------------------------------------------
        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: toolbarColumn.implicitHeight + Kirigami.Units.smallSpacing * 2
                color: Kirigami.Theme.backgroundColor
                Kirigami.Separator {
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                }

                ColumnLayout {
                    id: toolbarColumn
                    anchors {
                        left: parent.left; right: parent.right
                        verticalCenter: parent.verticalCenter
                        margins: Kirigami.Units.smallSpacing
                    }
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        Kirigami.Heading { text: "Tasks"; level: 2; Layout.fillWidth: true }
                        ToolButton {
                            icon.name: "view-refresh"
                            onClicked: full.reload()
                            enabled: !full.loading
                            ToolTip.text: "Refresh"
                            ToolTip.visible: hovered
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Label { text: "Project:" }
                        ComboBox {
                            id: filterCombo
                            Layout.fillWidth: true
                            model: ["All"].concat(full.lists)
                            onCurrentIndexChanged: {
                                if (full.allTasks.length > 0) { full.reload() }
                            }
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
                        spacing: Kirigami.Units.smallSpacing
                        TextField {
                            Layout.fillWidth: true
                            placeholderText: "Search…"
                            onTextChanged: { full.searchText = text; taskListModel.rebuild() }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Label { text: "Show:" }
                        CheckBox {
                            id: todoFilter
                            text: "Todo"; checked: true
                            onCheckedChanged: taskListModel.rebuild()
                        }
                        CheckBox {
                            id: doingFilter
                            text: "Doing"; checked: true
                            onCheckedChanged: taskListModel.rebuild()
                        }
                        CheckBox {
                            id: blockedFilter
                            text: "Blocked"; checked: true
                            onCheckedChanged: taskListModel.rebuild()
                        }
                        CheckBox {
                            id: validatingFilter
                            text: "Validating"; checked: true
                            onCheckedChanged: taskListModel.rebuild()
                        }
                        CheckBox {
                            id: doneFilter
                            text: "Done"; checked: false
                            onCheckedChanged: taskListModel.rebuild()
                        }
                    }
                }
            }

            Rectangle {
                visible: full.errorMsg !== ""
                Layout.fillWidth: true
                implicitHeight: visible ? errLabel.implicitHeight + Kirigami.Units.largeSpacing : 0
                color: Kirigami.Theme.negativeBackgroundColor
                Label {
                    id: errLabel
                    anchors { fill: parent; margins: Kirigami.Units.smallSpacing }
                    text: full.errorMsg
                    color: Kirigami.Theme.negativeTextColor
                    wrapMode: Text.WordWrap
                }
            }

            BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: full.loading
                visible: full.loading
            }

            ListView {
                id: listView
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: taskListModel
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                delegate: Loader {
                    width: listView.width
                    sourceComponent: model.isSectionHeader ? sectionHeaderComp : taskDelegateComp
                    property var itemModel: model
                }
            }

            Label {
                visible: !full.loading && taskListModel.count === 0 && full.errorMsg === ""
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                text: "No tasks found."
                opacity: 0.5
                padding: Kirigami.Units.gridUnit * 2
            }
        }

        Component {
            id: sectionHeaderComp
            Rectangle {
                height: Kirigami.Units.gridUnit * 1.5
                color: Kirigami.Theme.alternateBackgroundColor
                Label {
                    anchors {
                        left: parent.left; right: parent.right
                        verticalCenter: parent.verticalCenter
                        leftMargin: Kirigami.Units.largeSpacing
                    }
                    text: itemModel ? itemModel.sectionTitle : ""
                    font.bold: true
                    font.pixelSize: Kirigami.Units.gridUnit * 0.75
                    color: Kirigami.Theme.disabledTextColor
                }
            }
        }

        Component {
            id: taskDelegateComp
            TaskDelegate {
                width: listView.width
                taskId:       itemModel ? itemModel.id        : ""
                taskTitle:    itemModel ? itemModel.title     : ""
                taskStatus:   itemModel ? itemModel.status    : ""
                taskPriority: itemModel ? itemModel.priority  : ""
                taskList:     itemModel ? itemModel.list      : ""
                taskDue:      itemModel ? itemModel.due       : ""
                taskType:     itemModel ? itemModel.task_type : ""

                onStatusChangeRequested: function(newStatus) {
                    full.runCommand(
                        full.helper("set-status " + taskId + " " + newStatus),
                        function(_out) { full.reload() },
                        function(err)  { full.errorMsg = "set-status: " + err }
                    )
                }
            }
        }

        Component.onCompleted: {
            full.reload()
            full.watchSignal()
        }
    }
}
