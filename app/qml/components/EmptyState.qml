import QtQuick
import QtQuick.Layouts
import "../Palette.js" as P
Item {
    id: root
    property string title: "No activity yet"
    property string description: "Your activity will appear here automatically."
    property string symbol: "proofs"
    implicitWidth: 280
    implicitHeight: message.implicitHeight + 48
    Layout.fillWidth: true
    Layout.minimumWidth: 0
    Layout.minimumHeight: implicitHeight
    Column {
        id: message
        anchors.centerIn: parent
        width: Math.max(0, root.width - 40)
        spacing: 12
        Icon { name: root.symbol; tint: P.color("faint"); anchors.horizontalCenter: parent.horizontalCenter; width: 30; height: 30 }
        Text { text: root.title; color: P.color("text"); font.pixelSize: P.size("emptyTitle"); font.weight: Font.DemiBold; width: parent.width; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter }
        Text { text: root.description; color: P.color("muted"); font.pixelSize: P.size("caption"); lineHeight: 1.2; width: parent.width; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter }
    }
}
