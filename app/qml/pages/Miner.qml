import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "minerPage"
    heading: state.claiming ? "Claiming Rewards" : state.mining ? "Active Miner" : "Miner"
    subtitle: page.state.mining ? "" : page.state.miningBlocker || page.state.miningIssue || (page.state.pendingTransaction ? "A previous transaction will be reconciled before new work." : "Ready to start a mining session.")
    headerActions: [
        ActionButton { objectName: "startMiningButton"; visible: !page.state.mining; text: "Start Mining"; symbol: "play"; primary: true; enabled: page.state.miningBlocker.length===0 && !page.state.busy; onClicked: page.walletRequested("mine") },
        ActionButton { objectName: "pauseResumeMiningButton"; visible: page.state.mining && !page.state.claiming; text: page.state.manuallyPaused ? "Resume" : "Pause"; symbol: page.state.manuallyPaused ? "play" : "pause"; primary: true; onClicked: page.state.manuallyPaused ? page.state.resumeMining() : page.state.pauseMining() },
        ActionButton { objectName: "stopMiningButton"; visible: page.state.mining; text: "Stop"; symbol: "stop"; onClicked: page.state.stopMining() }
    ]
    GridLayout {
        Layout.fillWidth: true; columns: page.wide ? 2 : 1; columnSpacing: 16; rowSpacing: 16
        MiningCard { state: page.state; Layout.fillWidth: true; Layout.preferredHeight: Math.max(390,implicitHeight); Layout.preferredWidth: 920; Layout.minimumWidth: 0 }
        Card {
            Layout.fillWidth: true; Layout.preferredWidth: page.wide ? 360 : -1; Layout.maximumWidth: page.wide ? 360 : 100000; Layout.minimumWidth: page.wide ? 330 : 0
            title: "Session Summary"
            ColumnLayout {
                Layout.fillWidth: true; Layout.fillHeight: false; spacing: 8
                Repeater {
                    model: [
                        ["Start Time", page.state.sessionStarted],
                        ["Proofs Found", page.state.proofsFound.toString()],
                        ["Hashes computed", page.state.sessionHashes],
                        ["Proofs accepted", page.state.sessionAccepted.toString()],
                        ["Rejected proofs", page.state.sessionRejected.toString()],
                        ["Current epoch", page.state.epoch], ["Difficulty", page.state.difficulty],
                        ["Transaction", page.state.pendingTransaction ? "Pending" : "None pending"]
                    ]
                    delegate: RowLayout {
                        id: summaryLine
                        required property var modelData
                        Layout.fillWidth: true; Layout.preferredHeight: 22; Layout.maximumHeight: 22; spacing: 12
                        Text { text: summaryLine.modelData[0]; color: P.color("muted"); font.pixelSize: 13; Layout.fillWidth: true; Layout.preferredWidth: 0.9; Layout.minimumWidth: 0; elide: Text.ElideRight }
                        Text { text: summaryLine.modelData[1]; color: P.color("text"); font.pixelSize: 13; font.weight: Font.DemiBold; Layout.fillWidth: true; Layout.preferredWidth: 1.1; Layout.minimumWidth: 0; wrapMode: Text.NoWrap; elide: Text.ElideLeft; horizontalAlignment: Text.AlignRight; ToolTip.visible: !!page.Window.window && (summaryHover.hovered); ToolTip.text: summaryLine.modelData[0] + ": " + summaryLine.modelData[1]; HoverHandler { id: summaryHover } }
                    }
                }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: P.color("border") }
            RowLayout {
                Image { source: "qrc:/branding/vex.png"; Layout.preferredWidth: 46; Layout.preferredHeight: 46; fillMode: Image.PreserveAspectFit }
                Metric { label: "Wallet gas reserve"; value: page.state.nativeBalance; financial: true; unit: "VEX"; description: "Available wallet balance"; Layout.fillWidth: true }
            }
        }
    }
    Card {
        Layout.fillWidth: true; title: "Session Fees"
        subtitle: "Limits apply to proofs, beacons, epoch sealing and claims. Only included transactions incur fees."
        GridLayout {
            Layout.fillWidth: true; columns: page.wide ? 5 : 2; columnSpacing: 20; rowSpacing: 16
            Metric { label: page.state.mining ? "Active session limit" : page.state.sessionStarted !== "—" ? "Last session limit" : "Session fee limit"; value: page.state.approvedSessionBudget; financial: true; unit: "VEX"; Layout.fillWidth: true; Layout.preferredWidth: 1 }
            Metric { label: "Last transaction · fee limit"; value: page.state.lastTransactionFeeLimit; financial: true; precision: 6; unit: "VEX"; Layout.fillWidth: true; Layout.preferredWidth: 1 }
            Metric { label: "Actual fees spent"; value: page.state.sessionFeesSpent; financial: true; precision: 6; unit: "VEX"; Layout.fillWidth: true; Layout.preferredWidth: 1 }
            Metric { label: "Reserved for pending"; value: page.state.sessionFeesReserved; financial: true; precision: 6; unit: "VEX"; Layout.fillWidth: true; Layout.preferredWidth: 1 }
            Metric { label: "Remaining allowance"; value: page.state.sessionFeesRemaining; financial: true; precision: 6; unit: "VEX"; accent: true; Layout.fillWidth: true; Layout.preferredWidth: 1 }
        }
    }
    Card {
        Layout.fillWidth: true
        RowLayout {
            Layout.fillWidth: true
            Text { text: "Recent Mining Activity"; color: P.color("text"); font.pixelSize: 19; font.weight: Font.DemiBold; Layout.fillWidth: true }
            ActionButton { text: "View All"; compact: true; symbol: "arrow"; onClicked: page.state.page = "Proofs" }
        }
        ProofTable { state: page.state; rows: page.state.proofs; maximumRows: 5 }
    }
}
