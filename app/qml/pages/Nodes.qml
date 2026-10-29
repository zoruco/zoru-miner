import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "nodesPage"
    heading: "Nodes"
    subtitle: "Choose your RPC connection and follow network health."
    readonly property bool editingAllowed: !state.mining && !state.busy
    function addConnection() { if (state.addRpc(rpcInput.text)) rpcInput.clear() }
    Card {
        Layout.fillWidth: true
        title: "RPC Connections"
        GridLayout {
            columns: page.width > 650 ? 2 : 1
            Layout.fillWidth: true; columnSpacing: 12; rowSpacing: 10
            TextField {
                id: rpcInput; objectName: "addRpcUrl"
                Layout.fillWidth: true; Layout.minimumWidth: 0; implicitHeight: 43
                placeholderText: "https://your-rpc.example"; selectByMouse: true
                color: P.color("text"); placeholderTextColor: P.color("faint")
                background: Rectangle { color: P.color("background"); radius: 7; border.color: rpcInput.activeFocus ? P.color("orange") : P.color("border") }
                enabled: page.editingAllowed; Accessible.name: "New RPC URL"
                onAccepted: page.addConnection()
            }
            ActionButton { objectName: "addRpcButton"; text: "Add RPC"; symbol: "plus"; enabled: page.editingAllowed && rpcInput.text.trim().length > 0; onClicked: page.addConnection() }
        }
        Text { visible: !page.editingAllowed; text: "Stop mining and finish the wallet action to change connections."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        Repeater {
            model: page.state.nodes
            delegate: ColumnLayout {
                id: connectionRow
                required property var modelData
                Layout.fillWidth: true; spacing: 10
                RowLayout {
                    Layout.fillWidth: true; spacing: 12
                    RadioButton {
                        objectName: "selectRpc"
                        checked: connectionRow.modelData.selected
                        enabled: page.editingAllowed
                        Accessible.name: "Use " + connectionRow.modelData.url
                        onClicked: page.state.rpcUrl = connectionRow.modelData.url
                        implicitWidth: 28
                        indicator: Rectangle {
                            width: 20; height: 20; radius: 10
                            y: (parent.height-height)/2
                            color: "transparent"; border.width: 2
                            border.color: connectionRow.modelData.selected ? P.color("orange") : P.color("faint")
                            Rectangle { anchors.centerIn: parent; width: 10; height: 10; radius: 5; color: P.color("orange"); visible: connectionRow.modelData.selected }
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: 5
                        Text { objectName: "rpcUrlText"; text: connectionRow.modelData.url; color: P.color("text"); font.pixelSize: 14; Layout.fillWidth: true; wrapMode: Text.WrapAnywhere }
                        Text { visible: connectionRow.modelData.latencyMs !== undefined; text: visible ? connectionRow.modelData.latencyMs.toFixed(1) + " ms · Block " + connectionRow.modelData.height : ""; color: P.color("muted"); font.pixelSize: 12; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                    }
                    StatusChip { text: connectionRow.modelData.status; good: connectionRow.modelData.healthy === true }
                    ActionButton { objectName: "removeRpc"; text: "Remove"; compact: true; visible: page.state.nodes.length > 1; enabled: page.editingAllowed; onClicked: page.state.removeRpc(connectionRow.modelData.url) }
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: P.color("border") }
            }
        }
    }
    GridLayout {
        Layout.fillWidth: true; columns: page.wide ? 2 : 1; columnSpacing: 16; rowSpacing: 16
        Card {
            title: "Connection Response Time"; Layout.preferredWidth: 820; Layout.fillWidth: true; Layout.preferredHeight: Math.max(320,implicitHeight); Layout.minimumWidth: 400
            LineChart { Layout.fillWidth: true; Layout.fillHeight: true; unit: "ms"; samples: page.state.latencySamples; emptyText: "Measurements appear when the selected connection responds." }
        }
        NetworkCard { compact: true; state: page.state; Layout.fillWidth: true; Layout.preferredWidth: page.wide ? 470 : -1; Layout.minimumWidth: page.wide ? 470 : 0; Layout.maximumWidth: page.wide ? 470 : 100000 }
    }
    Card {
        Layout.fillWidth: true
        RowLayout {
            Layout.fillWidth: true
            Metric { label: "Mining Difficulty"; value: page.state.difficulty; Layout.fillWidth: true }
            Metric { label: "Mining Epoch"; value: page.state.epoch; Layout.fillWidth: true }
            Metric { label: "Native Gas Asset"; value: "VEX"; Layout.fillWidth: true }
        }
    }
}
