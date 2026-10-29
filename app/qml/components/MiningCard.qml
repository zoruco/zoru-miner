import QtQuick
import QtQuick.Layouts
import "../Palette.js" as P
Card {
    id: root
    required property var state
    GridLayout {
        columns: 2; columnSpacing: 14; rowSpacing: 8
        Layout.fillWidth: true
        Text { objectName: "miningCardTitle"; text: "Mining Status"; color: P.color("text"); font.pixelSize: 19; font.weight: Font.DemiBold; Layout.fillWidth: true; Layout.minimumWidth: implicitWidth; wrapMode: Text.NoWrap }
        StatusChip { Layout.maximumWidth: Math.max(80, root.width - root.padding * 2 - 160); text: root.state.miningStatus; good: root.state.activelyHashing }
        Text { objectName: "miningUptime"; text: (root.state.mining && !root.state.claiming ? "Session uptime  " : "Last session  ") + root.state.uptime; color: P.color("muted"); font.pixelSize: 12; Layout.columnSpan: 2; Layout.fillWidth: true; wrapMode: Text.NoWrap }
    }
    Text { text: !root.state.activelyHashing && root.state.lastMeasuredHashrate !== "—" ? "Last measured hashrate · CPU idle" : "Current hashrate"; color: P.color("muted"); font.pixelSize: 13; Layout.fillWidth: true }
    GridLayout {
        id: performance
        Layout.fillWidth: true; Layout.minimumWidth: 0
        columns: root.width >= Math.max(800, rateNumber.implicitWidth + rateUnit.implicitWidth + 364 + root.padding * 2) ? 2 : 1; columnSpacing: 18; rowSpacing: 14
        RowLayout {
            Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: 8
            Text { id: rateNumber; text: root.state.hashrateValue; color: P.color("orange"); font.pixelSize: root.width>650?59:43; font.weight: Font.Bold; Layout.minimumWidth:0 }
            Text { id: rateUnit; text: root.state.hashrateUnit; color: P.color("text"); font.pixelSize: 28; font.weight: Font.DemiBold }
            Item { Layout.fillWidth: true; Layout.minimumWidth: 0 }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.minimumWidth:0; Layout.preferredWidth:performance.columns===2?330:-1; spacing:14
            Metric { label:root.state.activelyHashing ? "Active threads" : "Configured threads";value:root.state.cpuThreads.toString();Layout.fillWidth:true;Layout.minimumWidth:0 }
            Metric { label:"Proofs accepted";value:root.state.sessionAccepted.toString();Layout.fillWidth:true;Layout.minimumWidth:0 }
            Metric { label:"Network latency";value:root.state.latency;Layout.fillWidth:true;Layout.minimumWidth:0 }
        }
    }
    Text { visible: root.state.mining || root.state.lastMeasuredHashrate !== "—"; text: root.state.mining ? root.state.miningExplanation : "Stopped · no CPU work is running."; color: P.color("muted"); font.pixelSize: 14; Layout.fillWidth: true; wrapMode: Text.WordWrap }
    Text { visible: root.state.lastMeasuredHashrate!=="—"; text: "Last measured: "+root.state.lastMeasuredHashrate+" · "+root.state.lastMeasuredAt; color: P.color("faint"); font.pixelSize: 12; Layout.fillWidth: true; wrapMode: Text.WordWrap }
    LineChart { implicitHeight: 180; Layout.fillWidth: true; Layout.fillHeight: true; samples: root.state.hashrateSamples; unit: "H/s"; emptyText: root.state.mining ? "Hashrate history appears after CPU work is measured." : "Start mining to see your hashrate history." }
}
