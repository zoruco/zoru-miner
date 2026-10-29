import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
ColumnLayout {
    id: root
    property string label: ""
    property string value: "—"
    property string description: ""
    property bool accent: false
    property bool financial: false
    property string unit: ""
    property int precision: 4
    readonly property string displayValue: (financial ? P.amountSummary(value, precision) : value) + (unit && value !== "—" ? " " + unit : "")
    Layout.minimumWidth: 0
    spacing: 6
    Text { text: root.label; color: P.color("muted"); font.pixelSize: P.size("caption"); Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight }
    Text {
        objectName: "metricValue"; text: root.displayValue
        color: root.accent ? P.color("orange") : P.color("text")
        font.pixelSize: P.size("value"); font.weight: Font.DemiBold
        fontSizeMode: Text.HorizontalFit; minimumPixelSize: 15
        Layout.fillWidth: true; Layout.minimumWidth: 0
        wrapMode: Text.NoWrap; elide: Text.ElideRight
        Accessible.name: root.label + ": " + root.value + (root.unit ? " " + root.unit : "")
        ToolTip.visible: !!root.Window.window && (valueHover.hovered)
        ToolTip.text: root.value + (root.unit ? " " + root.unit : "")
        ToolTip.delay: 250; ToolTip.timeout: 20000
        HoverHandler { id: valueHover }
    }
    Text { visible: root.description.length > 0; text: root.description; color: P.color("faint"); font.pixelSize: P.size("small"); lineHeight: 1.2; wrapMode: Text.WordWrap; Layout.fillWidth: true }
}
