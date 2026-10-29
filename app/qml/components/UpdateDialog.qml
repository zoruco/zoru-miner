import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
Dialog {
    id: root
    required property var state
    readonly property var updates: state.updates
    objectName: "softwareUpdateDialog"
    parent: Overlay.overlay; anchors.centerIn: parent; modal: true; focus: true
    width: Math.min(620,parent?parent.width-40:620); padding: 26
    // Keep the viewport independent of the content it lays out.
    height: Math.min(updates.notes.length>0?600:380,parent?parent.height-40:600)
    closePolicy: updates.installing ? Popup.NoAutoClose : Popup.CloseOnEscape
    background: Rectangle {color:P.color("panel");radius:16;border.color:P.color("border")}
    contentItem: ScrollView {
        id: formScroll
        implicitWidth: 0; clip: true; contentWidth: availableWidth; contentHeight: form.implicitHeight
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
        id: form
        width: formScroll.availableWidth
        spacing: 16
        Text { text: "Software Updates"; color: P.color("text"); font.pixelSize: 25; font.bold: true; Layout.fillWidth: true }
        Text { text: "Installed version: "+root.state.version; color: P.color("muted"); font.pixelSize: 14; Layout.fillWidth: true }
        Text { objectName: "updateMessage"; text: root.updates.message; color: root.updates.state==="Error"?P.color("yellow"):P.color("text"); font.pixelSize: 15; wrapMode: Text.WordWrap; Layout.fillWidth: true; textFormat: Text.PlainText }
        ProgressBar { objectName: "updateProgress"; visible: root.updates.state==="Downloading"; value: root.updates.progress; Layout.fillWidth: true }
        ScrollView {
            id: notesScroll
            implicitWidth: 0; contentWidth: availableWidth; contentHeight: releaseNotes.implicitHeight
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            visible: root.updates.notes.length>0; Layout.fillWidth: true; Layout.preferredHeight: visible?Math.min(180,releaseNotes.implicitHeight+16):0; clip: true
            Text { id: releaseNotes; objectName: "releaseNotes"; width: notesScroll.availableWidth; text: root.updates.notes; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: P.color("muted"); font.pixelSize: 14 }
        }
        Text { visible: root.updates.state==="Ready"; text: root.state.mining?"Installing ends this mining session. Your wallet and transaction records are kept.":"The system may ask for your administrator password. Your wallet password is not needed."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        Text { visible: root.updates.state==="Ready"&&!root.updates.canInstall; text: "For this build, open the download folder and run the installer."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        Text { text: "Downloads are saved in "+root.updates.downloadFolder; color: P.color("faint"); font.pixelSize: 12; wrapMode: Text.WrapAnywhere; Layout.fillWidth: true; textFormat: Text.PlainText }
        CheckBox { text: "Check for updates automatically"; checked: root.updates.automaticChecks; enabled: !root.updates.installing; onToggled: root.updates.automaticChecks=checked; palette.windowText: P.color("text") }
        Flow {
            Layout.fillWidth: true; spacing: 10
            ActionButton { objectName: "checkSoftwareUpdate"; text: "Check for Updates"; compact: true; enabled: !root.updates.busy&&!root.updates.installing; onClicked: root.updates.check() }
            ActionButton { objectName: "downloadSoftwareUpdate"; text: "Download Update"; primary: true; visible: root.updates.canDownload; onClicked: root.updates.download() }
            ActionButton { objectName: "installSoftwareUpdate"; text: "Install and Restart"; primary: true; visible: root.updates.state==="Ready"; enabled: root.updates.canInstall&&!root.state.busy; onClicked: root.state.installUpdate() }
            ActionButton { text: "Cancel Download"; compact: true; visible: root.updates.state==="Downloading"; onClicked: root.updates.cancelDownload() }
            ActionButton { text: "Open Download Folder"; compact: true; visible: root.updates.downloadFile.length>0; onClicked: root.updates.openDownloadFolder() }
            ActionButton { text: "Close"; compact: true; enabled: !root.updates.installing; onClicked: root.close() }
        }
        }
    }
}
