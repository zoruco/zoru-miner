import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "proofsPage"
    heading: "Proofs"
    subtitle: "View accepted proofs and their claim status from the canonical contract history."
    property string filter: "All"
    property var filtered: state.proofs.filter(function(row) {
        if (page.filter === "All") return true
        if (page.filter === "Reward paid") return P.rewardPaid(row)
        if (page.filter === "Needs attention") return P.failedProof(row) || ["Unknown", "Dropped", "Replaced"].indexOf(row.status) >= 0
        return !P.rewardPaid(row) && !P.failedProof(row) && row.status !== "Recovered"
    })
    Card {
        Layout.fillWidth: true
        RowLayout {
            Layout.fillWidth: true
            Text { text: page.state.hasWallet && !page.state.networkHealthy ? "Waiting for a verified connection" : page.state.historyStatus; color: P.color("muted"); font.pixelSize: 13; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            ComboBox { objectName: "proofFilter"; model: ["All", "Reward paid", "In progress", "Needs attention"]; onActivated: page.filter = currentText; implicitWidth: 180; implicitHeight: 39; Accessible.name: "Filter proofs" }
        }
        ProofTable { state: page.state; rows: page.filtered }
    }
}
