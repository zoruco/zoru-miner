import QtQuick
import QtQuick.Layouts
import "../Palette.js" as P
Rectangle {
    id: root
    property string title: ""
    property string subtitle: ""
    property int padding: 24
    default property alias content: body.data
    implicitWidth: 360
    Layout.minimumWidth: 0
    Layout.alignment: Qt.AlignTop
    radius: 13
    color: P.color("panel")
    gradient: Gradient {GradientStop {position:0;color:"#151a1f"} GradientStop {position:1;color:P.color("panel")}}
    border.color: P.color("border")
    implicitHeight: stack.implicitHeight + padding * 2
    ColumnLayout {
        id: stack
        anchors.fill: parent
        anchors.margins: root.padding
        spacing: 18
        ColumnLayout {
            visible: root.title.length > 0
            Layout.fillWidth: true
            spacing: 6
            Text { text: root.title; color: P.color("text"); font.pixelSize: P.size("section"); font.weight: Font.DemiBold; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            Text { visible: root.subtitle.length > 0; text: root.subtitle; color: P.color("muted"); font.pixelSize: P.size("caption"); lineHeight: 1.2; Layout.fillWidth: true; wrapMode: Text.WordWrap }
        }
        ColumnLayout { id: body; Layout.fillWidth: true; Layout.fillHeight: true; spacing: 16 }
    }
}
