// espulse — tray widget polling `espulse --json` (read-only queries, the same ones Bibimbap
// makes on connect). Polls every 5 min and when the popup opens; battery isn't decoded yet.
import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PC3
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as P5Support

PlasmoidItem {
    id: root

    property var st: null          // last good status from espulse --json
    property string err: "querying…"
    property string lastUpdate: ""

    // Runs through /bin/sh; Plasma's PATH often lacks the per-user bin dirs.
    readonly property string cmd: 'PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH" espulse --json'

    readonly property var activeStage: {
        if (!st || !st.stages) return null
        for (let s of st.stages) if (s.n === st.active_stage) return s
        return null
    }
    readonly property string dpiText: activeStage ? String(activeStage.x) : "–"
    readonly property string dpiShort: {
        if (!activeStage) return "–"
        const v = activeStage.x
        return v >= 1000 ? (v % 1000 === 0 ? v / 1000 + "k" : (v / 1000).toFixed(1) + "k") : String(v)
    }
    readonly property color stageColour: activeStage && activeStage.colour ? activeStage.colour : Kirigami.Theme.disabledTextColor

    P5Support.DataSource {
        id: exec
        engine: "executable"
        connectedSources: []
        onNewData: function (source, data) {
            disconnectSource(source)
            const code = data["exit code"]
            if (code === 127) {
                root.err = "espulse not installed"
                return
            }
            if (code !== 0) {
                const msg = (data["stderr"] || "").trim().split("\n")[0]
                root.err = msg.startsWith("Pulsar eS Xlite not found") ? "mouse not connected"
                    : msg.startsWith("No permission") ? "no permission (udev rule)"
                    : msg.startsWith("No reply") ? "no reply: close Bibimbap, or wake the mouse"
                    : msg !== "" ? msg : "mouse not reachable"
                return
            }
            try {
                root.st = JSON.parse(data["stdout"])
                root.err = ""
                root.lastUpdate = new Date().toLocaleTimeString(Qt.locale(), "hh:mm")
            } catch (e) {
                root.err = "unexpected output from espulse"
            }
        }
    }

    function refresh() { exec.connectSource(cmd) }

    Timer { interval: 5 * 60 * 1000; running: true; repeat: true; onTriggered: root.refresh() }
    Component.onCompleted: refresh()
    onExpandedChanged: { if (root.expanded) refresh() }

    Plasmoid.status: PlasmaCore.Types.ActiveStatus
    toolTipMainText: st ? ("Pulsar eS Xlite · " + dpiText + " DPI") : ("espulse: " + err)
    toolTipSubText: st
        ? ((st.polling_hz ? st.polling_hz + " Hz" : "polling ?") + " · " + st.mode + (lastUpdate ? " · updated " + lastUpdate : ""))
        : ""

    compactRepresentation: MouseArea {
        id: compactRoot
        implicitWidth: fullRow.implicitWidth + Kirigami.Units.smallSpacing * 2
        readonly property bool tight: width > 0 && width < fullRow.implicitWidth
        property bool wasExpanded: false
        onPressed: wasExpanded = root.expanded
        onClicked: root.expanded = !wasExpanded

        // Narrow tray slot: mouse icon, short DPI on top, stage colour dot in the corner.
        Item {
            anchors.fill: parent
            visible: compactRoot.tight
            Kirigami.Icon {
                anchors.fill: parent
                source: "input-mouse-symbolic"
                opacity: 0.45
            }
            PC3.Label {
                anchors.centerIn: parent
                text: root.dpiShort
                font.bold: true
                fontSizeMode: Text.Fit
                minimumPointSize: 6
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                style: Text.Outline
                styleColor: Kirigami.Theme.backgroundColor
            }
            Rectangle {
                width: Math.max(4, parent.width / 4); height: width; radius: width / 2
                anchors { right: parent.right; bottom: parent.bottom }
                color: root.stageColour
                visible: root.st !== null
            }
        }

        RowLayout {
            id: fullRow
            anchors.centerIn: parent
            visible: !compactRoot.tight
            spacing: Kirigami.Units.smallSpacing
            Rectangle {
                Layout.preferredWidth: Kirigami.Units.iconSizes.small / 2
                Layout.preferredHeight: Layout.preferredWidth
                radius: width / 2
                color: root.stageColour
            }
            PC3.Label { text: root.st ? root.dpiText + " DPI" : "–" }
        }
    }

    fullRepresentation: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Layout.minimumWidth: Kirigami.Units.gridUnit * 22
        Layout.minimumHeight: Kirigami.Units.gridUnit * 17

        component Pill: Rectangle {
            id: pill
            property string text
            property bool active: false
            property color swatch: "transparent"
            radius: height / 2
            color: active ? Kirigami.Theme.highlightColor : "transparent"
            border.width: 1
            border.color: active ? Kirigami.Theme.highlightColor : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.2)
            implicitWidth: pillRow.implicitWidth + Kirigami.Units.largeSpacing * 1.1
            implicitHeight: pillRow.implicitHeight + Kirigami.Units.smallSpacing * 1.4
            RowLayout {
                id: pillRow
                anchors.centerIn: parent
                spacing: Kirigami.Units.smallSpacing
                Rectangle {
                    visible: pill.swatch.a > 0
                    Layout.preferredWidth: Kirigami.Units.smallSpacing * 2
                    Layout.preferredHeight: Layout.preferredWidth
                    radius: width / 2
                    color: pill.swatch
                }
                PC3.Label {
                    text: pill.text
                    font.bold: pill.active
                    color: pill.active ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                    opacity: pill.active ? 1.0 : 0.7
                }
            }
        }

        component InfoRow: RowLayout {
            property string label
            property string value
            Layout.fillWidth: true
            PC3.Label { text: label; opacity: 0.75 }
            Item { Layout.fillWidth: true }
            PC3.Label { text: value; font.bold: true }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.largeSpacing
            Layout.leftMargin: Kirigami.Units.largeSpacing * 2
            Layout.rightMargin: Kirigami.Units.largeSpacing * 2
            Kirigami.Icon {
                source: root.st && root.st.mode === "wireless" ? "network-wireless-symbolic" : "network-wired-symbolic"
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
            }
            Kirigami.Heading { level: 4; text: "Pulsar eS Xlite" }
            Item { Layout.fillWidth: true }
            PC3.Label {
                text: root.st ? (root.st.mode === "wireless" ? "wireless" : "wired") + " · profile " + root.st.profile : ""
                opacity: 0.7
            }
        }

        PC3.Label {
            visible: root.err !== ""
            text: root.err
            opacity: 0.8
            Layout.alignment: Qt.AlignHCenter
        }

        ColumnLayout {
            visible: root.st !== null
            spacing: Kirigami.Units.smallSpacing
            Layout.alignment: Qt.AlignHCenter
            Kirigami.Heading { level: 5; text: "DPI stages"; opacity: 0.8; Layout.alignment: Qt.AlignHCenter }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: root.st ? root.st.stages : []
                    delegate: Pill {
                        required property var modelData
                        text: modelData.x === modelData.y ? String(modelData.x) : modelData.x + "×" + modelData.y
                        active: root.st && modelData.n === root.st.active_stage
                        swatch: modelData.colour ? modelData.colour : "transparent"
                    }
                }
            }
        }

        ColumnLayout {
            visible: root.st !== null
            spacing: Kirigami.Units.smallSpacing
            Layout.alignment: Qt.AlignHCenter
            Kirigami.Heading { level: 5; text: "Polling rate"; opacity: 0.8; Layout.alignment: Qt.AlignHCenter }
            RowLayout {
                spacing: Kirigami.Units.smallSpacing
                Layout.alignment: Qt.AlignHCenter
                Repeater {
                    model: [500, 1000, 2000, 4000, 8000]
                    delegate: Pill {
                        required property var modelData
                        text: modelData >= 1000 ? modelData / 1000 + "K" : String(modelData)
                        active: root.st && root.st.polling_hz === modelData
                    }
                }
            }
            PC3.Label {
                visible: root.st && root.st.polling_hz && !root.st.polling_hz_confirmed
                text: "(this rate's code is inferred, not yet confirmed)"
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                opacity: 0.6
                Layout.alignment: Qt.AlignHCenter
            }
        }

        ColumnLayout {
            visible: root.st !== null
            spacing: Kirigami.Units.smallSpacing / 2
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing * 2
            Layout.rightMargin: Kirigami.Units.largeSpacing * 2
            InfoRow { label: "Motion sync"; value: root.st && root.st.motion_sync ? "On" : "Off" }
            InfoRow { label: "Turbo mode"; value: root.st && root.st.turbo ? "On" : "Off" }
            InfoRow { label: "Angle snap"; value: root.st && root.st.angle_snap ? "On" : "Off" }
            InfoRow { label: "LOD"; value: root.st ? (root.st.lod === 2 ? "Default" : "level " + root.st.lod) : "" }
            InfoRow { label: "Debounce"; value: root.st ? (root.st.debounce_raw === 0 ? "0 ms" : "raw " + root.st.debounce_raw) : "" }
            InfoRow { label: "Rotation"; value: root.st && root.st.rotation ? root.st.rotation : "" }
            InfoRow {
                visible: root.st && root.st.mode === "wireless"
                label: "Dongle button · LED"
                value: root.st ? (root.st.dongle_button || "?") + " · " + (root.st.dongle_led ? "on" : "off") : ""
            }
            InfoRow { label: "Battery"; value: "not decoded yet" }
        }

        Item { Layout.fillHeight: true }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.bottomMargin: Kirigami.Units.smallSpacing
            PC3.Label {
                text: root.lastUpdate !== "" ? "updated " + root.lastUpdate : ""
                opacity: 0.6
            }
            PC3.ToolButton {
                icon.name: "view-refresh"
                text: "Refresh"
                onClicked: root.refresh()
            }
        }
    }
}
