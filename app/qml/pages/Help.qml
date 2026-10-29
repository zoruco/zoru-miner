import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "helpPage"
    heading: "Help"
    subtitle: "Set up your wallet, start mining and keep the app up to date."
    property string topic: "start"
    Flow {
        Layout.fillWidth: true; spacing: 10
        Repeater {
            model: [{key:"start",text:"Start mining"},{key:"protocol",text:"How ZORU works"},{key:"updates",text:"Software updates"}]
            delegate: ActionButton {
                required property var modelData
                text: modelData.text; primary: page.topic===modelData.key; compact: true
                onClicked: page.topic=modelData.key
            }
        }
    }
    Card {
        Layout.fillWidth: true
        Text {
            id: helpContent
            objectName: "helpContent"
            Layout.fillWidth: true; text: page.state.helpHtml(page.topic,P.color("orangeLight"),helpContent.font)
            textFormat: Text.RichText; wrapMode: Text.WordWrap
            color: P.color("text"); font.pixelSize: 15
            onLinkActivated: function(link) { if(link.indexOf("https://")===0)Qt.openUrlExternally(link) }
        }
    }
    RowLayout {
        Layout.fillWidth: true; spacing: 12
        ActionButton { text: "Open Wallet"; symbol: "wallet"; visible: page.topic==="start"; onClicked: {if(page.state.showIntroduction)page.state.completeIntroduction();page.state.page="Wallet"} }
        ActionButton { text: "Open Settings"; symbol: "settings"; visible: page.topic==="start"; onClicked: {if(page.state.showIntroduction)page.state.completeIntroduction();page.state.page="Settings"} }
        ActionButton { text: "Software Updates"; visible: page.topic==="updates"; onClicked: page.updateRequested() }
        Item { Layout.fillWidth: true }
    }
}
