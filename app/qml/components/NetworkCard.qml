import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
Card {
    id: root
    required property var state
    property bool compact: false
    title: compact ? "Asset Verification" : "Network & Asset Verification"
    subtitle: compact ? "" : "VEX EVM Mainnet · Chain 6736"
    StatusChip { visible: !root.compact; text: root.state.networkHealthy ? "Verified" : "Checking" }
    ColumnLayout {
        visible: root.compact; Layout.fillWidth: true; spacing: 16
        RowLayout {
            Layout.fillWidth:true; spacing:16
            Image {source:"qrc:/branding/token.png";Layout.preferredWidth:55;Layout.preferredHeight:55;fillMode:Image.PreserveAspectFit}
            ColumnLayout {Layout.fillWidth:true;spacing:5
                Text {text:"ZORU Token";color:P.color("text");font.pixelSize:18;font.bold:true}
                Text {text:root.state.networkHealthy?"ZORU · 8 decimals":"Checking mainnet";color:P.color("muted");font.pixelSize:13}
            }
            StatusChip {text:root.state.networkHealthy?"Verified":"Checking";good:root.state.networkHealthy}
        }
        Rectangle {Layout.fillWidth:true;height:1;color:P.color("border")}
        RowLayout {Layout.fillWidth:true
            Text {text:"Total Supply";color:P.color("muted");font.pixelSize:13;Layout.fillWidth:true}
            Text {text:P.amountSummary(root.state.totalSupply,4)+" ZORU";color:P.color("text");font.pixelSize:14;Layout.minimumWidth:0;elide:Text.ElideRight;ToolTip.visible:supplyHover.hovered;ToolTip.text:root.state.totalSupply+" ZORU";HoverHandler{id:supplyHover}}
        }
        ColumnLayout {Layout.fillWidth:true;spacing:8
            Text {text:"Token Address";color:P.color("muted");font.pixelSize:13;Layout.fillWidth:true}
            AddressText {objectName:"tokenFullAddress";text:root.state.contracts.length?root.state.contracts[0].address:"—"}
        }
        ActionButton {text:"View on Explorer";symbol:"external";Layout.fillWidth:true;onClicked:if(root.state.contracts.length)root.state.openExplorer("contract",root.state.contracts[0].address)}
    }
    Repeater {
        model: root.compact ? [] : root.state.contracts
        delegate: ColumnLayout {
            required property var modelData
            Layout.fillWidth: true; spacing: 8
            RowLayout {
                Layout.fillWidth: true
                Text { text: modelData.name; color: P.color("text"); font.pixelSize: 14; font.weight: Font.DemiBold; Layout.fillWidth: true }
            }
            RowLayout {
                Layout.fillWidth: true
                AddressText { objectName: "networkFullAddress"; text: modelData.address; color: P.color("muted") }
                ActionButton { text: "View"; compact: true; symbol: "external"; onClicked: root.state.openExplorer("contract",modelData.address) }
            }
        }
    }
    Rectangle { visible: !root.compact; Layout.fillWidth: true; height: 1; color: P.color("border") }
    RowLayout {
        visible: !root.compact
        Layout.fillWidth: true
        Metric { label: "ZORU Precision"; value: root.state.networkHealthy ? "8 decimals" : "—"; Layout.fillWidth: true }
        Metric { label: "Next epoch allocation"; value: root.state.currentReward; financial: true; precision: 8; unit: "ZORU"; Layout.fillWidth: true }
    }
}
