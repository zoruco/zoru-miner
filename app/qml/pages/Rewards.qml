import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "rewardsPage"
    heading: "Rewards"
    subtitle: "Accepted proofs share an epoch budget. Closed epochs can be sealed and claimed from this page."
    headerActions: [ActionButton { objectName: "claimRewardsButton";text: "Claim Rewards";primary: true;enabled: page.state.hasWallet&&page.state.networkHealthy&&!page.state.mining&&!page.state.busy;onClicked: page.walletRequested("claim") }]
    property var accepted: state.proofs.filter(function(row) { return P.rewardPaid(row) })
    GlobalStats { state: page.state; Layout.fillWidth: true }
    Card {
        title: "My Claimed Rewards"; Layout.fillWidth: true
        RowLayout {
            Layout.fillWidth: true; spacing: 20
            Image { source: "qrc:/branding/token.png"; Layout.preferredWidth: 68; Layout.preferredHeight: 68; fillMode: Image.PreserveAspectFit }
            ColumnLayout {
                Layout.fillWidth: true
                Metric { label: "Claimed ZORU"; value: page.state.confirmedRewards; financial: true; precision: 8; unit: "ZORU"; accent: true; Layout.fillWidth: true }
                Text { text: page.state.hasWallet && !page.state.networkHealthy ? "Waiting for a verified connection" : page.state.historyStatus; color: P.color("muted"); font.pixelSize: 13; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            }
        }
    }
    GridLayout {
        Layout.fillWidth: true
        columns: page.wide?2:1
        Metric { label: "My pending rewards";value: page.state.myPending;unit: "ZORU";financial: true;precision: 8;description: "Estimated share in unsealed epochs; it can change until sealing.";Layout.fillWidth: true }
        Metric { label: "My unclaimed rewards";value: page.state.myUnclaimed;unit: "ZORU";financial: true;precision: 8;description: "Rewards in sealed epochs, ready to claim.";Layout.fillWidth: true }
    }
    HalvingCard { state: page.state; Layout.fillWidth: true }
    GridLayout {
        Layout.fillWidth: true; columns: page.wide ? 2 : 1; columnSpacing: 16; rowSpacing: 16
        Card {
            Layout.fillWidth: true; Layout.preferredHeight: Math.max(310,implicitHeight); Layout.minimumWidth: 420
            title: "Observed Claimed Rewards"
            LineChart { Layout.fillWidth: true; Layout.fillHeight: true; samples: page.state.rewardSamples; unit: "ZORU"; emptyText: "Your confirmed rewards will build this chart." }
        }
        Card {
            Layout.fillWidth: true; Layout.preferredWidth: page.wide ? 390 : -1; Layout.preferredHeight: Math.max(310,implicitHeight)
            title: "Reward Summary"
            Metric { label: "Rewards paid"; value: page.state.confirmedRewards==="—"?"—":page.state.confirmedProofs.toString(); Layout.fillWidth: true }
            Metric { label: "Next epoch allocation"; value: page.state.currentReward; financial: true; precision: 8; unit: "ZORU"; Layout.fillWidth: true }

        }
    }
    Card {
        Layout.fillWidth: true; title: "Reward History"
        ProofTable { state: page.state; rows: page.accepted; rewards: true }
    }
}
