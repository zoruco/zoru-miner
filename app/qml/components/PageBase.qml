import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
ScrollView {
    id: root
    required property var state
    property string heading: ""
    property string subtitle: ""
    property bool wide: width >= 1000
    property alias headerActions: actions.data
    default property alias panels: body.data
    signal walletRequested(string action)
    signal navigate(string destination)
    signal updateRequested()
    clip: true
    contentWidth: availableWidth
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() {
            Qt.callLater(function() {
                var win=root.Window.window, item=win?win.activeFocusItem:null
                if(!item||!root.contentItem)return
                var cursor=item, belongs=false
                while(cursor){if(cursor===root){belongs=true;break}cursor=cursor.parent}
                if(!belongs)return
                var flick=root.contentItem as Flickable
                if(!flick)return
                var point=item.mapToItem(flick,0,0)
                if(point.y<12)flick.contentY=Math.max(0,flick.contentY+point.y-12)
                else if(point.y+item.height>root.availableHeight-12)flick.contentY+=point.y+item.height-root.availableHeight+12
            })
        }
    }
    Column {
        width: root.availableWidth
        spacing: 16
        Item { height: 1 }
        RowLayout {
            x: 26; width: parent.width - 52
            spacing: 18
            ColumnLayout {
                Layout.fillWidth: true; spacing: 7
                Text { objectName: "pageHeading"; text: root.heading; font.pixelSize: root.width >= 1100 ? P.size("page") : P.size("compactPage"); font.weight: Font.Bold; color: P.color("text"); Layout.fillWidth: true; wrapMode: Text.WordWrap }
                Text { visible: root.subtitle.length > 0; text: root.subtitle; color: P.color("muted"); font.pixelSize: root.width >= 1100 ? P.size("subtitle") : P.size("body"); lineHeight: 1.2; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            }
            Flow {
                id: actions
                visible: children.length > 0
                Layout.preferredWidth: root.width > 1100 ? 390 : 260
                Layout.maximumWidth: root.width > 1100 ? 390 : 260
                Layout.preferredHeight: implicitHeight
                layoutDirection: Qt.RightToLeft
                spacing: 10
            }
        }
        ColumnLayout { id: body; x: 26; width: parent.width - 52; spacing: 16 }
        Item { height: 26 }
    }
}
