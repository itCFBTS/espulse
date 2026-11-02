// espulse — tray widget. In the background it only asks `espulse --battery-json` (two or three read queries).
// The full read (`espulse --json`, the same queries Bibimbap makes on connect) runs only when
// the popup opens, and its battery value refreshes the icon too. Every 10 s it lists USB IDs from
// sysfs, which sends the mouse nothing, and reads the battery again if they changed.
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
    property int battery: -1      // -1 = unknown
    property bool powerConnected: false
    property bool outdated: false  // espulse doesn't know --battery-json
    property string runningCmd: "" // one espulse at a time: they'd share the mouse's reply buffer
    property string queuedCmd: ""
    property var usbIds: null      // eS product IDs last seen; null until the first check
    property int retries: 0        // battery reads left to try after a USB change

    // Runs through /bin/sh; Plasma's PATH often lacks the per-user bin dirs.
    readonly property string bin: 'PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH" espulse'
    readonly property string cmdFull: bin + " --json"
    readonly property string cmdBattery: bin + " --battery-json"
    readonly property string cmdUsb: 'for d in /sys/bus/usb/devices/*; do [ "$(cat "$d/idVendor" 2>/dev/null)" = 3710 ] && cat "$d/idProduct"; done | sort'
    readonly property color batteryColour: powerConnected ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
    // Top-down eS Xlite outline, Breeze symbolic style (recoloured to the theme's text colour).
    readonly property url mouseIcon: Qt.resolvedUrl("../icons/es-xlite.svg")

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
            if (source === root.cmdUsb) {
                const ids = (data["stdout"] || "").trim()
                if (root.usbIds !== null && ids !== root.usbIds) {
                    root.retries = 3
                    settle.restart()
                }
                root.usbIds = ids
                return
            }
            root.runningCmd = ""
            Qt.callLater(root.runQueued)
            const code = data["exit code"]
            if (source === root.cmdBattery) {
                let reading = null
                try {
                    reading = code === 0 ? JSON.parse(data["stdout"]) : null
                } catch (e) {
                    reading = null
                }
                root.outdated = code === 2
                root.battery = reading && typeof reading.battery_percent === "number" ? reading.battery_percent : -1
                root.powerConnected = reading ? reading.power_connected === true : false
                // The mouse takes a moment to reconnect after the cable comes out.
                if (root.battery >= 0) {
                    root.retries = 0
                } else if (root.retries > 0) {
                    root.retries--
                    settle.restart()
                }
                return
            }
            if (code !== 0) root.clearReading()
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
                if (typeof root.st.battery_percent === "number") root.battery = root.st.battery_percent
                root.powerConnected = root.st.power_connected === true
                root.err = ""
                root.lastUpdate = new Date().toLocaleTimeString(Qt.locale(), "hh:mm")
            } catch (e) {
                root.clearReading()
                root.err = "unexpected output from espulse"
            }
        }
    }

    function clearReading() {
        st = null
        battery = -1
        powerConnected = false
    }
    function run(cmd) {
        if (runningCmd !== "") {
            if (cmd === cmdFull || queuedCmd === "") queuedCmd = cmd
            return
        }
        runningCmd = cmd
        exec.connectSource(cmd)
    }
    function runQueued() {
        if (queuedCmd === "" || runningCmd !== "") return
        const cmd = queuedCmd
        queuedCmd = ""
        run(cmd)
    }
    function refresh() { run(cmdFull) }
    function refreshBattery() { run(cmdBattery) }

    Timer { interval: 5 * 60 * 1000; running: true; repeat: true; onTriggered: root.refreshBattery() }
    Timer { interval: 10 * 1000; running: true; repeat: true; triggeredOnStart: true; onTriggered: exec.connectSource(root.cmdUsb) }
    Timer { id: settle; interval: 3000; onTriggered: root.refreshBattery() }
    Component.onCompleted: refreshBattery()
    onExpandedChanged: { if (root.expanded) refresh() }

    Plasmoid.status: PlasmaCore.Types.ActiveStatus
    toolTipMainText: "Pulsar eS Xlite" + (battery >= 0 ? " · " + battery + "%" + (powerConnected ? " · plugged in" : "") : "")
    toolTipSubText: outdated ? "espulse is out of date: update it" : battery >= 0 ? "click for settings" : "battery unknown · click for settings"

    compactRepresentation: MouseArea {
        id: compactRoot
        implicitWidth: fullRow.implicitWidth + Kirigami.Units.smallSpacing * 2
        readonly property bool tight: width > 0 && width < fullRow.implicitWidth
        property bool wasExpanded: false
        onPressed: wasExpanded = root.expanded
        onClicked: root.expanded = !wasExpanded

        // Narrow tray slot: mouse icon, battery % on top.
        Item {
            anchors.fill: parent
            visible: compactRoot.tight
            Kirigami.Icon {
                anchors.fill: parent
                source: root.mouseIcon
                isMask: true
                color: Kirigami.Theme.textColor
                opacity: root.battery >= 0 ? 0.45 : 1.0
            }
            PC3.Label {
                anchors.centerIn: parent
                visible: root.battery >= 0
                text: root.battery
                color: root.batteryColour
                font.bold: true
                fontSizeMode: Text.Fit
                minimumPointSize: 6
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                style: Text.Outline
                styleColor: Kirigami.Theme.backgroundColor
            }
        }

        RowLayout {
            id: fullRow
            anchors.centerIn: parent
            visible: !compactRoot.tight
            spacing: Kirigami.Units.smallSpacing
            Kirigami.Icon {
                source: root.mouseIcon
                isMask: true
                color: Kirigami.Theme.textColor
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Kirigami.Units.iconSizes.small
            }
            PC3.Label {
                visible: root.battery >= 0
                text: root.battery + "%"
                color: root.batteryColour
            }
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
            InfoRow { label: "LOD"; value: root.st ? (typeof root.st.lod_mm === "number" ? root.st.lod_mm.toFixed(1) + " mm" : "code " + root.st.lod) : "" }
            InfoRow { label: "Debounce"; value: root.st ? (typeof root.st.debounce_ms === "number" ? root.st.debounce_ms + " ms" : "code " + root.st.debounce_raw) : "" }
            InfoRow { label: "Rotation"; value: root.st && root.st.rotation ? root.st.rotation : "" }
            InfoRow {
                visible: root.st && root.st.mode === "wireless"
                label: "Dongle button · LED"
                value: root.st ? (root.st.dongle_button || "?") + " · " + (root.st.dongle_led ? "on" : "off") : ""
            }
            // Battery and power from the same full read, so the plugged-in note can't outlive it.
            InfoRow {
                label: "Battery"
                value: root.st && typeof root.st.battery_percent === "number"
                    ? root.st.battery_percent + "%" + (root.st.power_connected ? " · plugged in" : "")
                    : (root.battery >= 0 ? root.battery + "%" : "unknown")
            }
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
