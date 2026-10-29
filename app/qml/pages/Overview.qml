import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../Palette.js" as P
PageBase {
    id: page
    objectName: "overviewPage"
    heading: "Overview"
    subtitle: "Your mining performance and recent activity."
    GlobalStats { state: page.state; Layout.fillWidth: true }
    GridLayout {
        Layout.fillWidth: true; columns: page.wide ? 2 : 1; columnSpacing: 16; rowSpacing: 16
        MiningCard { state: page.state; Layout.fillWidth: true; Layout.preferredWidth: 920; Layout.preferredHeight: Math.max(437,implicitHeight); Layout.minimumWidth: 450 }
        Card {
            title: "Your Balance"
            Layout.fillWidth: true; Layout.preferredWidth: page.wide ? 360 : -1; Layout.maximumWidth: page.wide ? 360 : 100000; Layout.minimumWidth: page.wide ? 360 : 0; Layout.preferredHeight: Math.max(437,implicitHeight)
            RowLayout {
                Layout.fillWidth: true; spacing: 17
                Image { source: "qrc:/branding/token.png"; fillMode: Image.PreserveAspectFit; Layout.preferredWidth: 67; Layout.preferredHeight: 67 }
                ColumnLayout {
                    Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: 5
                    Text { text: "ZORU"; color: P.color("muted"); font.pixelSize: 17 }
                    Text { text: P.amountSummary(page.state.tokenBalance, 4); color: P.color("text"); font.pixelSize: 31; minimumPixelSize: 18; fontSizeMode: Text.HorizontalFit; font.weight: Font.Bold; Layout.fillWidth: true; elide: Text.ElideRight; Accessible.name: page.state.tokenBalance + " ZORU"; ToolTip.visible: !!page.Window.window && (balanceHover.hovered); ToolTip.text: page.state.tokenBalance + " ZORU"; HoverHandler { id: balanceHover } }
                    Text { text: page.state.hasWallet ? "Live wallet balance" : "Set up your mining wallet"; color: P.color("faint"); font.pixelSize: 12; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: P.color("border") }
            RowLayout {
                spacing: 17; Layout.fillWidth: true
                Image { source: "qrc:/branding/vex.png"; fillMode: Image.PreserveAspectFit; Layout.preferredWidth: 52; Layout.preferredHeight: 52 }
                Metric { label: "Wallet gas reserve"; value: page.state.nativeBalance; financial: true; unit: "VEX"; description: "Available wallet balance · hover for full precision"; Layout.fillWidth: true }
            }
            Item { Layout.fillHeight: true }
        }
    }
    HalvingCard { state: page.state; Layout.fillWidth: true }
    Card {
        Layout.fillWidth: true
        RowLayout {
            Layout.fillWidth: true
            Text { text: "Recent Proof Activity"; color: P.color("text"); font.pixelSize: 19; font.weight: Font.DemiBold; Layout.fillWidth: true }
            ActionButton { text: "View All"; symbol: "arrow"; compact: true; onClicked: page.state.page = "Proofs" }
        }
        ProofTable { state: page.state; rows: page.state.proofs; maximumRows: 5 }
    }
}
