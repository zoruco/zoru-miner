import QtQuick
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "proofDetailPage"
    property var proof: state.selectedProof
    property string publicRecord: ["ZORU proof", "Epoch: "+(proof.epoch||"—"),
        "Status: "+(proof.status||"—"), "Proof ID: "+(proof.digest||"—"),
        "Submission: "+(proof.transactionHash||"—"), "Accepted block: "+(proof.blockNumber||"—"),
        "Claim block: "+(proof.claimBlock||"—"),
        "Reward: "+(P.rewardPaid(proof)?proof.reward+" ZORU":"Not claimed")].join("\n")
    heading: "Proof Detail"
    subtitle: "View the accepted proof and its claim status."
    headerActions: [
        ActionButton { text: "Back to Proofs";symbol: "back";onClicked: page.state.page="Proofs" },
        ActionButton { text: "Copy Details";symbol: "copy";onClicked: page.state.copyText(page.publicRecord) }
    ]
    Card {
        Layout.fillWidth: true
        title: page.proof.epoch ? "Epoch "+page.proof.epoch : "Proof not selected"
        StatusChip { text: page.proof.status||"Unavailable";explanation: page.state.transactionExplanation(page.proof) }
        Text { text: page.state.transactionExplanation(page.proof);color: P.color("muted");wrapMode: Text.WordWrap;font.pixelSize: 14;Layout.fillWidth: true }
        GridLayout {
            columns: page.wide?3:1
            columnSpacing: 20
            rowSpacing: 18
            Layout.fillWidth: true
            Metric { label: "Epoch start";value: page.state.displayTime(page.proof.timestamp||0);Layout.fillWidth: true }
            Metric { label: "Accepted block";value: page.proof.blockNumber||"—";Layout.fillWidth: true }
            Metric { label: "Reward claimed";value: P.rewardPaid(page.proof)?page.proof.reward:"Not claimed";unit: P.rewardPaid(page.proof)?"ZORU":"";financial: true;precision: 8;Layout.fillWidth: true }
        }
    }
    Card {
        Layout.fillWidth: true
        title: "Contract Record"
        Text { text: "Proof ID";color: P.color("muted");font.pixelSize: 13 }
        AddressText { text: page.proof.digest||"—" }
        Text { text: "Submission transaction";color: P.color("muted");font.pixelSize: 13 }
        AddressText { text: page.proof.transactionHash||"—" }
        ActionButton { visible: !!page.proof.transactionHash;text: "View Proof Submission";symbol: "external";onClicked: page.state.openExplorer("tx",page.proof.transactionHash) }
        ActionButton { visible: !!page.proof.claimBlock;text: "View Claim Block";symbol: "external";onClicked: page.state.openExplorer("block",page.proof.claimBlock) }
    }
}
