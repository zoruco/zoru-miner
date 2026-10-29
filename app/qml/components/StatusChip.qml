import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
Rectangle {
    id: root
    property string text: ""
    property string explanation: ""
    ToolTip.visible: !!root.Window.window && (statusHover.hovered && explanation.length > 0)
    ToolTip.text: explanation
    ToolTip.timeout: 20000
    HoverHandler { id: statusHover }
    property bool good: P.contains(["Accepted", "Finalized", "Reward paid", "Connected", "Verified", "Mining ready"], text)
    property bool bad: P.contains(["Rejected", "Preflight rejected", "Fee limit exceeded", "Reverted", "Stale", "Expired", "Failed", "Reorged", "Wrong network", "Network mismatch", "Offline"], text)
    property color tone: good ? P.color("green") : bad ? P.color("red") : P.color("yellow")
    implicitWidth: label.implicitWidth + 33
    implicitHeight: 29
    Layout.minimumWidth: 0
    clip: true
    Accessible.name: text
    radius: 7
    color: Qt.rgba(tone.r, tone.g, tone.b, 0.075)
    border.color: Qt.rgba(tone.r, tone.g, tone.b, 0.22)
    Rectangle { width: 7; height: 7; radius: 4; color: root.tone; anchors.left: parent.left; anchors.leftMargin: 10; anchors.verticalCenter: parent.verticalCenter }
    Text { id: label; text: root.text; color: root.tone; font.pixelSize: 12; anchors.left: parent.left; anchors.leftMargin: 23; anchors.right: parent.right; anchors.rightMargin: 10; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter }
}
