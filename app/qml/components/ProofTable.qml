import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
ColumnLayout {
    id: root
    required property var state
    property var rows: []
    property bool rewards: false
    property int maximumRows: 100
    readonly property var visibleRows: rows ? rows.slice(0, maximumRows) : []
    readonly property bool hasRows: visibleRows.length > 0
    readonly property real epochWidth: columnWidth("Epoch", function(row) { return row.epoch || "—" })
    readonly property real timeWidth: columnWidth("Epoch start", function(row) { return root.state.displayTime(row.timestamp || 0) })
    readonly property real proofMinimum: 185
    readonly property real rewardWidth: Math.min(220, columnWidth("Reward paid", rewardText))
    readonly property real statusWidth: Math.min(220, columnWidth("Status", function(row) { return row.status || "Pending" }) + 33)
    readonly property real minimumTableWidth: epochWidth + timeWidth + proofMinimum + statusWidth + rewardWidth + 64
    spacing: 0
    Layout.fillWidth: true
    function proofIdentifier(row) { return row.transactionHash || row.digest || "Not submitted" }
    function rewardText(row) { return P.rewardPaid(row) ? "+" + P.amountSummary(row.reward, 4) + " ZORU" : "—" }
    function reasonText(row) { return P.failedProof(row) ? (row.reason || root.state.transactionExplanation(row)) : "" }
    function columnWidth(heading, value) {
        var observedFont = measure.font
        var width = measure.advanceWidth(heading)
        for (var i = 0; i < visibleRows.length; ++i)
            width = Math.max(width, measure.advanceWidth(value(visibleRows[i])))
        return Math.ceil(width) + 12
    }
    Text { id: fontSample; visible: false; font.pixelSize: 13 }
    FontMetrics { id: measure; font: fontSample.font }
    Flickable {
        id: viewport
        objectName: "proofTableViewport"
        visible: root.hasRows
        Layout.fillWidth: true
        implicitHeight: body.height + (contentWidth > width ? 18 : 0)
        contentWidth: Math.max(width, root.minimumTableWidth)
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded; Accessible.name: "Scroll proof columns" }
        Column {
            id: body
            width: viewport.contentWidth
            readonly property real proofWidth: width - root.epochWidth - root.timeWidth - root.statusWidth - root.rewardWidth - 64
            Row {
                objectName: "proofTableHeader"
                height: 35; spacing: 16
                Repeater {
                    model: ["Epoch", "Epoch start", "Transaction / proof", "Status", "Reward paid"]
                    delegate: Text {
                        required property int index
                        required property string modelData
                        text: modelData; color: P.color("muted"); font.pixelSize: 12
                        width: [root.epochWidth, root.timeWidth, body.proofWidth, root.statusWidth, root.rewardWidth][index]
                        height: parent.height; verticalAlignment: Text.AlignVCenter
                        wrapMode: Text.NoWrap; maximumLineCount: 1
                    }
                }
            }
            Rectangle { objectName: "proofTableRule"; width: parent.width; height: 1; color: P.color("border") }
            Repeater {
                model: root.visibleRows
                delegate: Button {
                    id: rowButton
                    required property var modelData
                    width: body.width; height: root.reasonText(modelData) ? 67 : 51
                    leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                    hoverEnabled: true
                    Accessible.name: "Open proof for epoch " + (modelData.epoch || "pending")
                    Accessible.description: (modelData.status || "Pending") + ". " + root.state.transactionExplanation(modelData)
                    onClicked: root.state.selectProof(modelData.id)
                    background: Rectangle {
                        color: rowButton.hovered ? "#1b2228" : "transparent"
                        border.width: rowButton.visualFocus ? 1 : 0; border.color: P.color("orange")
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: "#222a31" }
                    }
                    contentItem: Row {
                        spacing: 16
                        Text { objectName: "proofEpochCell"; text: rowButton.modelData.epoch || "—"; color: P.color("text"); font.pixelSize: 13; width: root.epochWidth; height: rowButton.height; verticalAlignment: Text.AlignVCenter; wrapMode: Text.NoWrap; maximumLineCount: 1 }
                        Text { objectName: "proofTimeCell"; text: root.state.displayTime(rowButton.modelData.timestamp || 0); color: P.color("muted"); font.pixelSize: 13; width: root.timeWidth; height: rowButton.height; verticalAlignment: Text.AlignVCenter; wrapMode: Text.NoWrap; maximumLineCount: 1 }
                        Column {
                            width: body.proofWidth; height: rowButton.height; spacing: 4; topPadding: root.reasonText(rowButton.modelData) ? 14 : 17
                            Text { objectName: "proofIdentifierCell"; text: root.proofIdentifier(rowButton.modelData); color: P.color("text"); font.pixelSize: 13; width: parent.width; wrapMode: Text.NoWrap; maximumLineCount: 1; elide: Text.ElideMiddle; ToolTip.visible: !!root.Window.window && (identifierHover.hovered); ToolTip.text: text; ToolTip.timeout: 20000; HoverHandler { id: identifierHover } }
                            Text { visible: text.length > 0; text: root.reasonText(rowButton.modelData); color: P.color("muted"); font.pixelSize: 11; width: parent.width; elide: Text.ElideRight; ToolTip.visible: !!root.Window.window && (reasonHover.hovered); ToolTip.text: text; ToolTip.timeout: 20000; HoverHandler { id: reasonHover } }
                        }
                        Item { width: root.statusWidth; height: rowButton.height; StatusChip { text: rowButton.modelData.status || "Pending"; explanation: root.state.transactionExplanation(rowButton.modelData); width: parent.width; anchors.verticalCenter: parent.verticalCenter } }
                        Text { text: root.rewardText(rowButton.modelData); color: P.rewardPaid(rowButton.modelData) ? P.color("orange") : P.color("faint"); font.pixelSize: 13; width: root.rewardWidth; height: rowButton.height; verticalAlignment: Text.AlignVCenter; wrapMode: Text.NoWrap; maximumLineCount: 1; elide: Text.ElideRight; ToolTip.visible: !!root.Window.window && (rewardHover.hovered && P.rewardPaid(rowButton.modelData)); ToolTip.text: rowButton.modelData.reward + " ZORU"; HoverHandler { id: rewardHover } }
                    }
                }
            }
        }
    }
    EmptyState { objectName: "proofTableEmpty"; visible: !root.hasRows; title: root.rewards ? "No rewards yet" : "No proofs yet"; description: !root.state.hasWallet ? "Create or import a wallet to get started." : root.rewards ? "Confirmed mining rewards will appear here." : "Start a mining session to see your proofs." }
}
