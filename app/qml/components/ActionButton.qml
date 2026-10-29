import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
Button {
    id: root
    property string symbol: ""
    property bool primary: false
    property bool danger: false
    property bool compact: false
    hoverEnabled: true
    implicitHeight: compact ? 38 : 48
    implicitWidth: Math.max(compact ? 42 : 110, contentItem.implicitWidth + 36)
    leftPadding: 18; rightPadding: 18
    opacity: enabled ? 1 : 0.42
    Accessible.name: text
    contentItem: RowLayout {
        spacing: 10
        Icon { visible: root.symbol.length > 0; name: root.symbol; tint: root.primary ? "#111314" : root.danger ? P.color("red") : P.color("text"); Layout.preferredWidth: 18; Layout.preferredHeight: 18 }
        Text { text: root.text; color: root.primary ? "#111314" : root.danger ? P.color("red") : P.color("text"); font.pixelSize: root.compact ? 12 : 14; font.weight: Font.DemiBold; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; Layout.fillWidth: true }
    }
    background: Rectangle {
        radius: 9
        color: root.primary ? (root.down ? "#f06b16" : root.hovered ? "#ff9a46" : P.color("orange")) : (root.hovered ? "#222a32" : P.color("elevated"))
        border.width: root.visualFocus ? 2 : 1
        border.color: root.visualFocus ? P.color("orangeLight") : root.primary ? "#ff9d51" : root.danger ? "#7b323b" : P.color("border")
    }
}
