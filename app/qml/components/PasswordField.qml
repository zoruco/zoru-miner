import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
TextField {
    id: root
    property bool passwordVisible: false
    implicitWidth: 240
    Layout.minimumWidth: 0
    implicitHeight: 47
    echoMode: passwordVisible ? TextInput.Normal : TextInput.Password
    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
    maximumLength: 1024
    selectByMouse: true
    color: P.color("text")
    placeholderTextColor: P.color("faint")
    font.pixelSize: 14
    leftPadding: 14; rightPadding: 53
    onVisibleChanged: if(!visible) {text="";passwordVisible=false}
    background: Rectangle { radius: 8; color: "#0d1317"; border.width: root.activeFocus ? 2 : 1; border.color: root.activeFocus ? P.color("orange") : P.color("border") }
    ToolButton {
        id: eye
        objectName: root.objectName + "Eye"
        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
        width: 45; height: parent.height
        Accessible.name: root.passwordVisible ? "Hide password" : "Show password"
        onClicked: {root.passwordVisible=!root.passwordVisible;root.forceActiveFocus()}
        contentItem: Icon { name: root.passwordVisible ? "eyeOff" : "eye"; tint: P.color("muted") }
        padding: 13
        background: Rectangle { color: "transparent"; radius: 7; border.color: eye.visualFocus ? P.color("orange") : "transparent" }
    }
}
