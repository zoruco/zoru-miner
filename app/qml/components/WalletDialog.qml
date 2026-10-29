import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../Palette.js" as P
Dialog {
    id: root
    required property var state
    property string mode: "unlock"
    property string destination: ""
    property string error: ""
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(550, parent ? parent.width-50 : 550)
    modal: true; focus: true; padding: 27
    closePolicy: state.busy ? Popup.NoAutoClose : Popup.CloseOnEscape
    function openAction(action,file) {mode=action;destination=file||"";error="";password.text="";confirmation.text="";acknowledge.checked=false;password.passwordVisible=false;open()}
    function submit() {
        if(state.busy)return
        error=""
        if(mode==="reset"){state.resetSettings();close();return}
        if(mode!=="archive"&&!password.text){error="Enter your wallet password.";password.forceActiveFocus();return}
        if(mode==="mine"||mode==="claim"){
            if(!acknowledge.checked){error="Confirm the session fee authorization before starting.";return}
            if(!(mode==="claim"?state.claimRewards(password.text,true):state.startMining(password.text,true)))error="Check your wallet setup, backup, and network before starting."
            return
        }
        if(mode==="archive"||mode==="remove"){
            var phrase=mode==="archive"?"RESTORE WALLET":"REMOVE WALLET"
            if(!acknowledge.checked||confirmation.text!==phrase){error="Confirm your recovery backup and type "+phrase+".";return}
            state.walletAction(mode,password.text,phrase)
        }else if(mode==="acknowledge"){
            if(!acknowledge.checked){error="Confirm that you have a separate recovery backup.";return}
            state.walletAction(mode,password.text)
        }else state.walletAction(mode,password.text,"",destination)
    }
    onOpened: {if(mode!=="reset"&&mode!=="archive")password.forceActiveFocus();else confirmation.forceActiveFocus()}
    onClosed: {password.text="";password.passwordVisible=false;confirmation.text="";acknowledge.checked=false;error="";destination=""}
    Connections {
        target: root.state
        function onWalletActionFinished(action,success,message) {
            if(action!==root.mode && !(["mine","claim"].indexOf(root.mode)>=0&&action==="authorize"))return
            password.text="";password.passwordVisible=false
            if(success)root.close();else{root.error=message;password.forceActiveFocus()}
        }
    }
    background: Rectangle { color: P.color("panel"); radius: 16; border.color: P.color("border") }
    contentItem: ColumnLayout {
        spacing: 17
        Text { text: root.mode==="reveal"?"View Recovery Details":root.mode==="claim"?"Claim Rewards":root.mode==="mine"?"Start Mainnet Mining":root.mode==="backup"?"Export Encrypted Backup":root.mode==="remove"?"Remove Local Wallet":root.mode==="archive"?"Restore Wallet Access":root.mode==="reset"?"Reset Preferences":root.mode==="acknowledge"?"Confirm Recovery Backup":"Unlock Wallet"; color: P.color("text"); font.pixelSize: 25; font.weight: Font.Bold; Layout.fillWidth: true; wrapMode: Text.WordWrap }
        Text {
            text: root.mode==="reveal"?"Enter your wallet password. Make sure nobody can see your screen. Recovery details provide full access to your funds.":["mine","claim"].indexOf(root.mode)>=0?"Authorize up to "+root.state.sessionBudget+" VEX in total gas fees for this session. Proof submission, beacon publication, sealing and claims can spend gas. Included transactions can spend VEX even if they revert. Mining in this interface does not claim rewards automatically. The backend stops before a new transaction would exceed your remaining allowance.":root.mode==="backup"?"Your password is required to save the encrypted wallet. Keep the backup separate from this device.":root.mode==="remove"?"This removes the encrypted wallet on this device. Your funds and external backups remain unchanged. Without your recovery details, access may be lost.":root.mode==="archive"?"Your current encrypted wallet will be preserved locally before you import recovery details. No funds are transferred. You need an external backup, recovery phrase, or private key to restore access.":root.mode==="reset"?"Restore default app preferences. Your encrypted wallet, external backups, and mining activity will stay unchanged.":root.mode==="acknowledge"?"Confirm that you have a separate encrypted backup, recovery phrase, or private key. Never share those details.":"Enter the password that protects your local wallet."
            color: P.color("muted"); font.pixelSize: 14; Layout.fillWidth: true; wrapMode: Text.WordWrap; lineHeight: 1.15
        }
        Text { visible: root.mode==="mine"&&root.state.pendingTransaction; text: "A previous transaction is awaiting settlement. This session reconciles the signed transaction before preparing new work."; color: P.color("yellow"); font.pixelSize: 13; Layout.fillWidth: true; wrapMode: Text.WordWrap }
        PasswordField { id: password; objectName: "dialogPassword"; visible: root.mode!=="reset"&&root.mode!=="archive"; Layout.fillWidth: true; placeholderText: "Wallet password"; Accessible.name: "Wallet password"; onAccepted: root.submit() }
        CheckBox {
            id: acknowledge; objectName: "dialogAuthorization"; visible: P.contains(["mine","claim","remove","archive","acknowledge"], root.mode)
            Layout.fillWidth: true
            text: ["mine","claim"].indexOf(root.mode)>=0?"I authorize mainnet transactions within this session's fee limit.":"I have the recovery details needed to restore access."
            contentItem: Text { text: acknowledge.text; color: P.color("text"); font.pixelSize: 13; wrapMode: Text.WordWrap; leftPadding: acknowledge.indicator.width+acknowledge.spacing; verticalAlignment: Text.AlignVCenter }
        }
        TextField { id: confirmation; objectName: "dialogConfirmation"; visible: root.mode==="remove"||root.mode==="archive"; Layout.fillWidth: true; implicitHeight: 44; placeholderText: root.mode==="remove"?"Type REMOVE WALLET":"Type RESTORE WALLET"; Accessible.name: "Wallet action confirmation"; onAccepted: root.submit() }
        Text { visible: root.error.length>0; text: root.error; color: P.color("red"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            Item { Layout.fillWidth: true }
            ActionButton { objectName: "dialogCancel"; text: "Cancel"; enabled: !root.state.busy; onClicked: root.close() }
            ActionButton { objectName: "dialogContinue"; text: root.state.busy?"Working…":root.mode==="claim"?"Claim Rewards":root.mode==="mine"?"Start Mining":root.mode==="reset"?"Reset Preferences":"Continue"; primary: root.mode!=="remove"; danger: root.mode==="remove"; enabled: !root.state.busy; onClicked: root.submit() }
        }
    }
}
