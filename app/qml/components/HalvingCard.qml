import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
Card {
    id: root
    property var state
    property var stats: state ? state.halving : ({})
    title: stats.issuanceEnded ? "Issuance Complete" : "Reward Halving"
    function amount(key) { return stats[key] === undefined ? "—" : String(stats[key]) }
    GridLayout {
        columns: root.width > 1000 ? 4 : root.width > 520 ? 2 : 1
        Layout.fillWidth: true; columnSpacing: 20; rowSpacing: 18
        Metric { label: "Next Epoch Allocation"; value: root.amount("currentReward"); financial: true; precision: 8; unit: "ZORU"; accent: true; Layout.fillWidth: true; Layout.preferredWidth: 1 }
        Metric { label: "Following Era Allocation"; value: root.amount("nextReward"); financial: true; precision: 8; unit: "ZORU"; Layout.fillWidth: true; Layout.preferredWidth: 1 }
        Metric { label: "Opened Epochs Until Halving"; value: root.stats.epochsUntilHalving === undefined ? "—" : String(root.stats.epochsUntilHalving); Layout.fillWidth: true; Layout.preferredWidth: 1 }
        Metric { label: "Halving Era"; value: root.stats.halvingEra === undefined ? "—" : String(root.stats.halvingEra); Layout.fillWidth: true; Layout.preferredWidth: 1 }
        Metric { label: "Maximum Supply"; value: root.amount("maximumSupply"); financial: true; precision: 8; unit: "ZORU"; Layout.fillWidth: true; Layout.preferredWidth: 1 }
        Metric { label: "Lifetime Minted"; value: root.amount("lifetimeMinted"); financial: true; precision: 8; unit: "ZORU"; Layout.fillWidth: true; Layout.preferredWidth: 1 }
    }
    Text { text: "Halving Progress"; color: P.color("muted"); font.pixelSize: 13 }
    ProgressBar { Layout.fillWidth: true; from: 0; to: Number(root.stats.halvingEpochs || 1); value: root.stats.issuanceEnded ? to : Number(root.stats.halvingProgressEpochs || 0) }
    Text {
        text: root.stats.issuanceEnded ? "All scheduled rewards have been issued."
              : root.stats.nextReward === "0.00000000" ? "Issuance ends after this era. No remaining supply is allocated."
              : "Progress counts allocated epochs from the contract emission index. The epoch budget is shared among accepted proofs."
        color: P.color("faint"); font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true
    }
}
