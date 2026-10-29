import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "securityPage"
    heading: "Security"
    subtitle: "Manage your password, backups, and recovery."
    property string passwordError: ""
    Connections {
        target: page.state
        function onWalletActionFinished(action,success,message) {if(action!=="password")return;currentPassword.text="";nextPassword.text="";confirmPassword.text="";page.passwordError=success?"":message}
    }
    GridLayout {
        Layout.fillWidth: true; columns: page.wide ? 2 : 1; columnSpacing: 16; rowSpacing: 16
        Card {
            title: "Wallet Protection"; Layout.fillWidth: true; Layout.minimumWidth: 380
            RowLayout {
                Layout.fillWidth: true; spacing: 18
                Image { source: "qrc:/branding/token.png"; Layout.preferredWidth: 58; Layout.preferredHeight: 58; fillMode: Image.PreserveAspectFit }
                ColumnLayout {
                    Layout.fillWidth: true
                    Text { text: page.state.address ? page.state.shortAddress(page.state.address) : "Set up your wallet"; color: P.color("text"); font.pixelSize: 18; font.weight: Font.DemiBold }
                    Text { text: page.state.hasWallet ? "Encrypted on this device" : "Create or import a wallet to set up security."; color: P.color("muted"); font.pixelSize: 13; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                }
                StatusChip { text: page.state.walletDamaged ? "Recovery needed" : page.state.hasWallet ? "Protected" : "Not set up"; good: page.state.hasWallet && !page.state.walletDamaged; bad: page.state.walletDamaged }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: P.color("border") }
            Metric { label: "Lock wallet on close"; value: "Always on"; description: "Mining stops and your wallet locks when the app closes."; Layout.fillWidth: true }
            Metric { label: "Transaction approval"; value: "Password required"; description: "Approve a fee limit before each session."; Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Auto-lock when mining is stopped"; color: P.color("text"); font.pixelSize: 14; Layout.fillWidth: true }
                ComboBox { objectName: "autoLockChoice"; model: ["1 minute","5 minutes","15 minutes","30 minutes"]; currentIndex: [1,5,15,30].indexOf(page.state.autoLockMinutes); onActivated: page.state.autoLockMinutes=[1,5,15,30][currentIndex]; implicitWidth: 160; implicitHeight: 42; Accessible.name: "Auto-lock when mining is stopped" }
            }
        }
        Card {
            title: "Change Password"; subtitle: "Choose a new password for the wallet on this device."; Layout.fillWidth: true
            PasswordField { id: currentPassword; objectName: "currentPassword"; Layout.fillWidth: true; placeholderText: "Current password"; Accessible.name: "Current wallet password" }
            PasswordField { id: nextPassword; objectName: "nextPassword"; Layout.fillWidth: true; placeholderText: "New password · at least 12 characters"; Accessible.name: "New wallet password" }
            PasswordField { id: confirmPassword; objectName: "confirmNewPassword"; Layout.fillWidth: true; placeholderText: "Confirm new password"; Accessible.name: "Confirm new wallet password" }
            Text { visible: page.passwordError.length > 0; text: page.passwordError; color: P.color("red"); font.pixelSize: 13; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            ActionButton {
                objectName: "changePasswordButton"; text: page.state.busy ? "Updating…" : "Update Password"; primary: true; Layout.alignment: Qt.AlignRight; enabled: page.state.hasWallet && !page.state.mining && !page.state.busy
                onClicked: {if(nextPassword.text.length<12||nextPassword.text!==confirmPassword.text){page.passwordError="Use at least 12 characters and matching new passwords.";return}page.passwordError="";page.state.walletAction("password",nextPassword.text,"","",currentPassword.text)}
            }
        }
        Card {
            title: "Encrypted Backup"; subtitle: "Save a protected copy in a separate, safe location."; Layout.fillWidth: true
            Text { text: page.state.backupConfirmed ? "Your recovery backup is confirmed. Keep it safe and make a new copy after changing your password." : "An external backup is required before mining. Your password is needed to export it."; color: P.color("muted"); font.pixelSize: 14; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            Flow {
                Layout.fillWidth: true; spacing: 10
                ActionButton { text: "View Recovery Details"; symbol: "eye"; enabled: page.state.hasWallet && !page.state.busy; onClicked: page.walletRequested("reveal") }
                ActionButton { text: "Export Backup"; symbol: "download"; enabled: page.state.hasWallet; onClicked: page.walletRequested("backup") }
                ActionButton { visible: page.state.hasWallet && !page.state.backupConfirmed; text: "I Have a Recovery Backup"; compact: true; onClicked: page.walletRequested("acknowledge") }
            }
        }
        Card {
            title: "Wallet Recovery"; subtitle: "Restore an encrypted backup, recovery phrase, or private key."; Layout.fillWidth: true
            Text { text: "If you cannot unlock your wallet, recovery preserves the existing encrypted file before you import your recovery details. Pending mining payments must be resolved first."; color: P.color("muted"); font.pixelSize: 14; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            ActionButton { text: "Restore Wallet Access"; symbol: "security"; enabled: page.state.hasWallet && !page.state.mining && !page.state.busy; onClicked: page.walletRequested("archive") }
        }
    }
    Card {
        title: "Remove Local Wallet"; Layout.fillWidth: true
        RowLayout {
            Layout.fillWidth: true
            Text { text: "Remove only the encrypted wallet stored on this device. Your on-chain assets and external backups are unchanged. Your password and a confirmation are required."; color: P.color("muted"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            ActionButton { text: "Remove Wallet"; danger: true; enabled: page.state.hasWallet && !page.state.mining && !page.state.busy; onClicked: page.walletRequested("remove") }
        }
    }
}
