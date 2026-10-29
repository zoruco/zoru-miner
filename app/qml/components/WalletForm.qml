import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import "../Palette.js" as P
Card {
    id: root
    objectName: "walletSetup"
    required property var state
    property string mode: "create"
    property string method: "recovery_phrase"
    property string selectedFile: ""
    property string error: ""
    title: "Set Up Wallet"
    subtitle: "Keep control of your funds with an encrypted wallet on this device."
    function clearSecrets() { secret.text="";originalPassword.text="";password.text="";confirmPassword.text="";password.passwordVisible=false;confirmPassword.passwordVisible=false;originalPassword.passwordVisible=false;secret.passwordVisible=false }
    function submit() {
        if(state.busy)return
        error=""
        if(password.text.length<12 || password.text!==confirmPassword.text){error="Use at least 12 characters and matching passwords.";return}
        if(!acknowledge.checked){error="Confirm that you understand wallet recovery before continuing.";return}
        if(mode==="create")state.walletAction("create",password.text)
        else if(method==="keystore"){
            if(!selectedFile || !originalPassword.text){error="Choose your encrypted file and enter its original password.";return}
            state.walletAction("import",password.text,"",selectedFile,originalPassword.text)
        }else{
            if(!secret.text.trim()){error="Enter your recovery phrase or private key.";return}
            state.walletAction("recovery",password.text,"","","",secret.text.trim(),method)
        }
    }
    Connections {
        target: root.state
        function onWalletActionFinished(action,success,message) {
            if(["create","import","recovery"].indexOf(action)<0)return
            root.clearSecrets();if(!success)root.error=message
        }
    }
    Component.onDestruction: clearSecrets()
    RowLayout {
        Layout.fillWidth: true
        ActionButton { objectName: "createTab"; text: "Create New Wallet"; primary: root.mode === "create"; Layout.fillWidth: true; onClicked: {root.mode="create";root.error="";root.clearSecrets()} }
        ActionButton { objectName: "importTab"; text: "Import Existing Wallet"; primary: root.mode === "import"; Layout.fillWidth: true; onClicked: {root.mode="import";root.error="";root.clearSecrets()} }
    }
    ComboBox {
        visible: root.mode === "import"; Layout.fillWidth: true; implicitHeight: 42
        objectName: "importMethod"
        model: ["Recovery phrase", "Private key", "Encrypted wallet file"]
        onActivated: {root.method=["recovery_phrase","private_key","keystore"][currentIndex];root.clearSecrets();root.error=""}
        Accessible.name: "Wallet import method"
    }
    Text { visible: root.mode === "import" && root.method !== "keystore"; text: root.method === "private_key" ? "Private key" : "Recovery phrase"; color: P.color("text"); font.pixelSize: 14 }
    PasswordField { id: secret; objectName: "recoverySecret"; visible: root.mode === "import" && root.method !== "keystore"; Layout.fillWidth: true; maximumLength: 2048; placeholderText: root.method === "private_key" ? "Your EVM private key" : "Your recovery words, in order"; Accessible.name: root.method === "private_key" ? "Private key" : "Recovery phrase" }
    ActionButton { visible: root.mode === "import" && root.method === "keystore"; Layout.fillWidth: true; text: root.selectedFile ? root.selectedFile.split('/').pop() : "Choose Encrypted Wallet"; symbol: "wallet"; onClicked: chooser.open() }
    Text { visible: root.mode === "import" && root.method === "keystore"; text: "Original file password"; color: P.color("text"); font.pixelSize: 14 }
    PasswordField { id: originalPassword; objectName: "originalFilePassword"; visible: root.mode === "import" && root.method === "keystore"; Layout.fillWidth: true; placeholderText: "Password used to encrypt the imported file"; Accessible.name: "Original keystore password" }
    Text { text: "New local password"; color: P.color("text"); font.pixelSize: 14 }
    PasswordField { id: password; objectName: "newWalletPassword"; Layout.fillWidth: true; placeholderText: "At least 12 characters"; Accessible.name: "New local password"; onAccepted: confirmPassword.forceActiveFocus() }
    RowLayout {
        Layout.fillWidth: true; spacing: 7
        Repeater { model: 4; delegate: Rectangle { required property int index; Layout.fillWidth: true; height: 5; radius: 3; color: root.state.passwordStrength(password.text) > index ? P.color("green") : P.color("border") } }
        Text { text: password.text.length === 0 ? "" : root.state.passwordStrength(password.text) >= 3 ? "Strong" : password.text.length < 12 ? "Too short" : "Good"; color: password.text.length >= 12 ? P.color("green") : P.color("muted"); font.pixelSize: 12; Layout.preferredWidth: 62; horizontalAlignment: Text.AlignRight }
    }
    Text { text: "Confirm local password"; color: P.color("text"); font.pixelSize: 14 }
    PasswordField { id: confirmPassword; objectName: "confirmWalletPassword"; Layout.fillWidth: true; placeholderText: "Repeat your new local password"; Accessible.name: "Confirm local password"; onAccepted: root.submit() }
    CheckBox {
        id: acknowledge; objectName: "walletRecoveryAcknowledged"; Layout.fillWidth: true
        text: "I understand that my password and recovery backup are needed to restore access."
        contentItem: Text { text: acknowledge.text; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; leftPadding: acknowledge.indicator.width + acknowledge.spacing; verticalAlignment: Text.AlignVCenter }
    }
    Text { visible: root.error.length > 0; text: root.error; color: P.color("red"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
    ActionButton { objectName: "walletContinue"; text: root.state.busy ? "Protecting Wallet…" : root.mode === "create" ? "Create Wallet" : "Import Wallet"; symbol: "arrow"; primary: true; Layout.fillWidth: true; enabled: !root.state.busy; onClicked: root.submit() }
    Text { text: "Your recovery details stay on this device. Keep an encrypted backup in a safe, separate location."; color: P.color("faint"); font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }
    FileDialog { id: chooser; title: "Choose encrypted wallet"; fileMode: FileDialog.OpenFile; nameFilters: ["Encrypted wallets (*.json)"]; onAccepted: root.selectedFile=String(selectedFile) }
}
