import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "settingsPage"
    heading: "Settings"
    subtitle: "Set CPU usage, fee limits and notifications."
    GridLayout {
        Layout.fillWidth: true; columns: page.wide ? 2 : 1; columnSpacing: 16; rowSpacing: 16
        Card {
            Layout.fillWidth: true; title: "Default Miner Settings"
            Text { text: page.state.cpuModel; color: P.color("text"); font.pixelSize: 14; font.weight: Font.DemiBold; wrapMode: Text.WordWrap; Layout.fillWidth: true; Layout.minimumWidth: 0 }
            Text { text: page.state.maxThreads+" threads available · suggested "+page.state.suggestedThreads; color: P.color("muted"); font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                Text { text: "CPU threads"; color: P.color("text"); font.pixelSize: 14; Layout.fillWidth: true }
                ComboBox { objectName: "cpuThreads"; model: page.state.maxThreads===4?[1,2,4]:page.state.maxThreads===2?[1,2]:[1]; currentIndex: model.indexOf(page.state.cpuThreads); onActivated: page.state.cpuThreads=model[currentIndex]; enabled: !page.state.mining; implicitWidth: 150; implicitHeight: 43; Accessible.name: "CPU mining threads" }
            }
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Compute intensity"; color: P.color("text"); font.pixelSize: 14; Layout.fillWidth: true }
                ComboBox { objectName: "computeIntensity"; model: ["Eco · 25%","Balanced · 75%","Performance · 100%"]; currentIndex: page.state.intensity <= 25 ? 0 : page.state.intensity < 100 ? 1 : 2; onActivated: page.state.intensity=[25,75,100][currentIndex]; enabled: !page.state.mining; implicitWidth: 190; implicitHeight: 43; Accessible.name: "CPU intensity" }
            }
            Text { visible: page.state.mining; text: "Stop mining to change CPU settings."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Stop after first accepted proof"; color: P.color("text"); font.pixelSize: 14; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Switch { objectName: "stopAfterProofToggle"; checked: page.state.stopAfterProof; enabled: !page.state.mining && !page.state.busy; onToggled: page.state.stopAfterProof=checked; Accessible.name: "Stop after first accepted proof" }
            }
            Text { text: "Ends the session after one on-chain ProofAccepted receipt. Rewards are claimed separately in Rewards."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        }
        Card {
            Layout.fillWidth: true; title: "Mining Fee Limits"
            Metric { label: "Safety limit per transaction"; value: (page.state.globalStats.transactionCap || "—")+" VEX"; description: "Actual fees vary. This cap is the most one transaction may spend. Your session limit covers all transactions in that session."; Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Maximum per session (VEX)"; color: P.color("text"); font.pixelSize: 14; Layout.fillWidth: true }
                TextField { id: budget; objectName: "sessionBudget"; text: page.state.sessionBudget; implicitWidth: 180; implicitHeight: 43; maximumLength: 79; enabled: !page.state.mining; validator: RegularExpressionValidator { regularExpression: /(?:0|[1-9][0-9]{0,59})(?:\.[0-9]{1,18})?/ } Accessible.name: "Session fee limit in VEX" }
            }
            ActionButton { text: "Save Fee Limit"; compact: true; enabled: !page.state.mining; onClicked: page.state.sessionBudget=budget.text; Layout.alignment: Qt.AlignRight }
        }
        Card {
            Layout.fillWidth: true; title: "Notifications"
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Activity notifications"; color: P.color("text"); font.pixelSize: 14; Layout.fillWidth: true }
                Switch { objectName: "notificationsToggle"; checked: page.state.notificationsEnabled; onToggled: page.state.notificationsEnabled=checked; Accessible.name: "Activity notifications" }
            }
            Text { text: "Wallet and security alerts stay on."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        }
    }
    Card {
        Layout.fillWidth: true; title: "Software Updates"
        Text { text: "Installed version: "+page.state.version+" · "+page.state.updates.message; color: P.color("muted"); font.pixelSize: 14; wrapMode: Text.WordWrap; Layout.fillWidth: true; textFormat: Text.PlainText }
        ActionButton { objectName: "openSoftwareUpdates"; text: "Software Updates"; onClicked: page.updateRequested(); Layout.alignment: Qt.AlignRight }
    }
    Card {
        Layout.fillWidth: true; title: "Reset Preferences"
        RowLayout {
            Layout.fillWidth: true
            Text { text: "Restore default CPU, fee, connection, notification, and inactivity settings. Your encrypted wallet, backups, and mining history stay unchanged."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            ActionButton { text: "Reset Preferences"; danger: true; enabled: !page.state.mining && !page.state.busy; onClicked: page.walletRequested("reset") }
        }
    }
}
