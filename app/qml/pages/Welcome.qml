import QtQuick
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "welcomePage"
    heading: ""
    subtitle: ""
    Item {
        Layout.fillWidth: true; implicitHeight: Math.max(page.width > 1100 ? 350 : 290,welcomeIntro.implicitHeight+28)
        Image { source: "qrc:/branding/app.png"; anchors.right: parent.right; anchors.top: parent.top; width: parent.width*0.55; height: parent.height+100; fillMode: Image.PreserveAspectFit; opacity: 0.075 }
        Column {
            id: welcomeIntro
            anchors.left: parent.left; anchors.top: parent.top; anchors.leftMargin: 40; width: parent.width > 1050 ? 680 : parent.width-70; spacing: 18
            Text { text: "WELCOME"; color: P.color("faint"); font.pixelSize: 17; font.letterSpacing: 1.2 }
            Column {
                spacing: 0
                Text { text: "Welcome to"; color: P.color("text"); font.pixelSize: page.width > 1100 ? 65 : 50; font.weight: Font.Bold }
                Text { text: "Zoru Miner"; color: P.color("orange"); font.pixelSize: page.width > 1100 ? 65 : 50; font.weight: Font.Bold }
            }
            Text { text: "Mine ZORU with your CPU on VEX EVM Mainnet."; color: P.color("muted"); font.pixelSize: 19; width: parent.width; wrapMode: Text.WordWrap; lineHeight: 1.2 }
        }
    }
    GridLayout {
        Layout.fillWidth: true; Layout.leftMargin: 35; Layout.rightMargin: 35; columns: page.width>=900?3:1; columnSpacing: 30; rowSpacing: 28
        Repeater {
            model: [{title:"Connect Wallet",description:"Create or restore your wallet to receive ZORU rewards.",symbol:"wallet"},{title:"Configure Miner",description:"Choose your CPU usage and a clear session fee limit.",symbol:"settings"},{title:"Start Mining",description:"Approve your session and follow every proof.",symbol:"play"}]
            delegate: ColumnLayout {
                required property var modelData
                required property int index
                Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: 17
                Rectangle {
                    Layout.alignment: Qt.AlignHCenter; width: 100; height: 100; radius: 50; color: "#0e1418"; border.color: P.color("border"); border.width: 2
                    Icon { name: modelData.symbol; tint: P.color("orange"); anchors.centerIn: parent; width: 38; height: 38 }
                    Rectangle { width: 31; height: 31; radius: 16; color: P.color("elevated"); border.color: P.color("faint"); anchors.horizontalCenter: parent.horizontalCenter; y: -13; Text { text: (index+1).toString(); color: P.color("text"); font.pixelSize: 15; anchors.centerIn: parent } }
                }
                Text { text: modelData.title; color: P.color("text"); font.pixelSize: 20; font.weight: Font.DemiBold; Layout.fillWidth: true; Layout.minimumWidth: 0; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap }
                Text { text: modelData.description; color: P.color("muted"); font.pixelSize: 16; Layout.fillWidth: true; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter }
            }
        }
    }
    Item { Layout.preferredHeight: 20 }
    RowLayout {
        Layout.alignment: Qt.AlignHCenter; spacing: 24
        ActionButton { objectName: "getStarted"; text: "Get Started"; symbol: "arrow"; primary: true; implicitWidth: 290; implicitHeight: 62; onClicked: page.state.completeIntroduction() }
        ActionButton { text: "Learn More"; symbol: "external"; implicitWidth: 240; implicitHeight: 62; onClicked: page.state.openGuide() }
    }
}
