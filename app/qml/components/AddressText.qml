import QtQuick
import QtQuick.Layouts
import "../Palette.js" as P
TextEdit {
    readOnly: true
    selectByMouse: true
    textFormat: TextEdit.PlainText
    wrapMode: TextEdit.WrapAnywhere
    color: P.color("text")
    font.family: "monospace"
    font.pixelSize: 13
    Layout.fillWidth: true
    Layout.minimumWidth: 0
    Accessible.name: text
}
