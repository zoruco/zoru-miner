import QtQuick
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "walletPage"
    heading: state.hasWallet ? "Wallet" : "Wallet & Network Setup"
    subtitle: state.hasWallet ? "Manage your wallet and gas reserve securely." : "Create or restore your wallet, then get ready to mine."
    GridLayout {
        Layout.fillWidth: true; columns: page.wide ? 2 : 1; columnSpacing: 16; rowSpacing: 16
        WalletForm { visible: !page.state.hasWallet; state: page.state; Layout.fillWidth: true; Layout.minimumWidth: 430 }
        Card {
            visible: page.state.hasWallet; Layout.fillWidth: true; Layout.minimumWidth: 400
            title: "ZORU Wallet"
            RowLayout {
                Layout.fillWidth: true; spacing: 20
                Image { source: "qrc:/branding/token.png"; Layout.preferredWidth: 70; Layout.preferredHeight: 70; fillMode: Image.PreserveAspectFit }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 7
                    AddressText { objectName: "walletFullAddress"; text: page.state.address || "Unlock to verify your wallet"; font.pixelSize: 17 }
                    StatusChip { text: page.state.walletDamaged ? "Recovery needed" : page.state.unlocked ? "Unlocked" : "Locked"; good: page.state.unlocked; bad: page.state.walletDamaged }
                }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: P.color("border") }
            Metric { label: "ZORU balance"; value: page.state.tokenBalance; financial: true; unit: "ZORU"; precision: 8; Layout.fillWidth: true; accent: true }
            Metric { label: "Wallet gas reserve"; value: page.state.nativeBalance; financial: true; unit: "VEX"; description: "Wallet balance available for fees. Hover over an amount for full precision."; Layout.fillWidth: true }
            Text { text: page.state.backupConfirmed ? "An external recovery backup has been confirmed." : "Save an encrypted backup before you begin mining."; color: page.state.backupConfirmed ? P.color("green") : P.color("yellow"); font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            Flow {
                Layout.fillWidth: true; spacing: 10
                ActionButton { visible: !page.state.unlocked; text: "Unlock Wallet"; primary: true; symbol: "lock"; onClicked: page.walletRequested("unlock") }
                ActionButton { visible: !!page.state.address; text: "Copy Address"; symbol: "copy"; onClicked: page.state.copyText(page.state.address) }
                ActionButton { visible: page.state.unlocked; text: "Lock"; symbol: "lock"; onClicked: page.state.lockWallet() }
                ActionButton { text: "Back Up"; symbol: "download"; onClicked: page.walletRequested("backup") }
            }
        }
        NetworkCard { state: page.state; Layout.fillWidth: true; Layout.minimumWidth: 330 }
    }
    Card {
        visible: page.state.walletDamaged; Layout.fillWidth: true; title: "Restore Wallet Access"
        Text { text: "The encrypted wallet could not be opened. Recovery preserves the existing encrypted file before you import a backup, recovery phrase, or private key."; color: P.color("muted"); font.pixelSize: 14; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        ActionButton { text: "Wallet Recovery"; symbol: "security"; onClicked: page.walletRequested("archive") }
    }
}
