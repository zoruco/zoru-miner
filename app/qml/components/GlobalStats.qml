import QtQuick
import QtQuick.Layouts
import "../Palette.js" as P
Card {
    id: root
    required property var state
    title: "Global Contract Data"
    subtitle: "A verified snapshot at EVM block "+state.height+". All amounts are network totals."
    GridLayout {
        columns: root.width>950?4:root.width>560?2:1
        Layout.fillWidth: true
        columnSpacing: 20
        rowSpacing: 18
        Repeater {
            model: [["Allocated rewards","allocated","ZORU"],["Funded rewards","funded","ZORU"],
                    ["Rewards paid","paid","ZORU"],["Pending epoch rewards","pending","ZORU"],
                    ["Unclaimed rewards","unclaimed","ZORU"],["Total accepted proofs","totalProofs",""],
                    ["Current epoch proofs","epochProofs",""],["Current epoch budget","epochBudget","ZORU"]]
            delegate: Metric {
                required property var modelData
                label: modelData[0]
                value: root.state.networkHealthy?(root.state.globalStats[modelData[1]]||"—"):"—"
                unit: modelData[2]
                financial: modelData[2]==="ZORU"
                precision: 8
                Layout.fillWidth: true
                Layout.preferredWidth: 1
            }
        }
    }
    Text {
        text: root.state.networkHealthy?
            (root.state.globalStats.admissionPaused?"Proof admission paused":"Proof admission active")+" · "+
            (root.state.globalStats.claimsPaused?"Claims paused":"Claims active"):"Waiting for a verified contract snapshot"
        color: P.color("muted")
        font.pixelSize: 12
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }
}
