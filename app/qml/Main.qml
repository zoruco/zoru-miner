import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import QtQuick.Dialogs
import "components"
import "Palette.js" as P
ApplicationWindow {
    id: root
    objectName: "mainWindow"
    required property var state
    width: 1586; height: 992
    minimumWidth: Math.min(960, Screen.desktopAvailableWidth); minimumHeight: Math.min(680, Screen.desktopAvailableHeight)
    visible: true
    title: "ZORU Miner — v1.5.4 UI Mainnet Preview (read-only)"
    color: "transparent"
    flags: Qt.Window | Qt.FramelessWindowHint | Qt.WindowMinMaxButtonsHint | Qt.WindowCloseButtonHint
    palette.window: P.color("background")
    palette.windowText: P.color("text")
    palette.base: "#0e1419"
    palette.text: P.color("text")
    palette.button: P.color("elevated")
    palette.buttonText: P.color("text")
    palette.highlight: P.color("orange")
    palette.highlightedText: "#101315"
    property string loadedPage: ""
    property string toast: ""
    property bool toastError: false
    property bool booting: true
    readonly property int sidebarWidth: width >= 1250 ? 220 : 180
    readonly property bool expanded: visibility === Window.Maximized || visibility === Window.FullScreen
    function toggleMaximize() {if(visibility===Window.Maximized)showNormal();else showMaximized()}
    function showWallet(action) {
        if(state.busy)return
        if(action==="backup")backupChooser.open()
        else walletDialog.openAction(action,"")
    }
    function loadPage() {
        var target=state.showIntroduction&&state.page!=="Help"?"Welcome":state.page
        if(loadedPage===target)return
        loadedPage=target
        pageLoader.setSource("pages/"+target+".qml", {state:root.state})
    }
    Component.onCompleted: loadPage()
    Connections {
        target: root.state
        function onNavigationChanged() {root.loadPage()}
        function onNotification(message,error) {root.toast=message;root.toastError=error;toastTimer.restart()}
        function onWalletChanged() {if(!root.state.unlocked){recoveryDialog.close();if(["mine","claim"].indexOf(walletDialog.mode)>=0)walletDialog.close()}}
        function onRecoveryDetails(kind,secret) {recoveryDialog.kind=kind==="private_key"?"Private key":kind;recoveryDialog.secret=secret;recoveryDialog.error="";recoveryDialog.open();recoveryTimer.restart()}
    }
    Timer { id: toastTimer; interval: 5500; onTriggered: root.toast="" }
    Timer { interval: 460; running: true; onTriggered: root.booting=false }
    Rectangle {
        objectName: "windowSurface"
        anchors.fill: root.contentItem; anchors.margins: root.expanded ? 0 : 10; radius: root.expanded ? 0 : 12; color: P.color("background"); border.color: "#363e45"; border.width: root.expanded ? 0 : 1; clip: true
        RowLayout {
            anchors.fill: parent; anchors.margins: root.expanded ? 0 : 1; spacing: 0
            Rectangle {
                Layout.preferredWidth: root.sidebarWidth; Layout.fillHeight: true; color: "#101519"
                Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: P.color("border") }
                ColumnLayout {
                    anchors.fill: parent; spacing: 0
                    Item {
                        Layout.fillWidth: true; Layout.preferredHeight: root.height >= 850 ? 190 : root.height < 680 ? 100 : 145
                        Column {
                            anchors.centerIn: parent; spacing: 8
                            Image { source: "qrc:/branding/app.png"; width: root.height >= 850 ? 116 : root.height < 680 ? 52 : 82; height: width; fillMode: Image.PreserveAspectFit; anchors.horizontalCenter: parent.horizontalCenter }
                            Text { text: "<b>ZORU</b> <font color='#727f8d'>MINER</font>"; textFormat: Text.RichText; color: P.color("text"); font.pixelSize: root.sidebarWidth > 190 ? 21 : 17; anchors.horizontalCenter: parent.horizontalCenter }
                        }
                    }
                    ScrollView {
                        id: navigationScroll
                        objectName: "navigationScroll"
                        Layout.fillWidth: true; Layout.fillHeight: true
                        clip: true
                        contentWidth: availableWidth
                        contentHeight: navigationColumn.implicitHeight
                        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        ColumnLayout {
                            id: navigationColumn
                            width: navigationScroll.availableWidth
                            spacing: 0
                    Repeater {
                        model: [{name:"Overview",icon:"overview"},{name:"Miner",icon:"miner"},{name:"Rewards",icon:"rewards"},{name:"Wallet",icon:"wallet"},{name:"Proofs",icon:"proofs"},{name:"Nodes",icon:"nodes"},{name:"Security",icon:"security"},{name:"Settings",icon:"settings"},{name:"Help",icon:"help"}]
                        delegate: Button {
                            id: navigationButton
                            required property var modelData
                            readonly property bool selected: root.loadedPage===modelData.name || (root.loadedPage==="Welcome"&&modelData.name==="Overview") || (root.loadedPage==="ProofDetail"&&modelData.name==="Proofs")
                            objectName: "nav"+modelData.name
                            Layout.fillWidth: true; Layout.preferredHeight: root.height >= 850 ? 70 : 57; Layout.rightMargin: 7
                            leftPadding: root.sidebarWidth > 190 ? 33 : 23
                            hoverEnabled: true
                            Accessible.name: modelData.name
                            onClicked: {if(root.state.showIntroduction&&modelData.name!=="Help")root.state.completeIntroduction();root.state.page=modelData.name;root.loadPage()}
                            background: Rectangle {
                                color: navigationButton.selected ? "#29211d" : navigationButton.hovered ? "#182027" : "transparent"; radius: 10
                                border.color: navigationButton.visualFocus ? P.color("orange") : "transparent"
                                Rectangle { visible: navigationButton.selected; width: 4; height: parent.height; color: P.color("orange") }
                            }
                            contentItem: RowLayout {
                                spacing: root.sidebarWidth > 190 ? 22 : 15
                                Icon { name: navigationButton.modelData.icon; tint: navigationButton.selected ? P.color("orange") : P.color("muted"); Layout.preferredWidth: 26; Layout.preferredHeight: 26 }
                                Text { text: navigationButton.modelData.name; color: navigationButton.selected ? P.color("orangeLight") : P.color("muted"); font.pixelSize: 15; Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter }
                            }
                        }
                    }

                        }
                    }
                    ActionButton { text: "Update available"; visible: root.state.updates.canDownload||root.state.updates.state==="Ready"; compact: true; Layout.alignment: Qt.AlignHCenter; onClicked: updateDialog.open() }
                    Text { text: "ZORU Miner " + root.state.version + " Preview"; color: P.color("faint"); font.pixelSize: 11; Layout.alignment: Qt.AlignHCenter; Layout.bottomMargin: 18; Layout.topMargin: 8 }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true; Layout.fillHeight: true; spacing: 0
                Rectangle {
                    id: topbar
                    Layout.fillWidth: true; Layout.preferredHeight: 67; color: "#11161a"
                    DragHandler { target: null; onActiveChanged: if(active)root.startSystemMove() }
                    TapHandler { acceptedButtons: Qt.LeftButton; onDoubleTapped: root.toggleMaximize() }
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 22; anchors.rightMargin: 16; spacing: root.width > 1200 ? 24 : 12
                        Item { Layout.fillWidth: true }
                        Rectangle { width: 9; height: 9; radius: 5; color: root.state.networkHealthy && (!root.state.mining || root.state.miningHolds.length === 0) ? P.color("green") : P.color("yellow") }
                        Text { text: root.width > 1200 ? "VEX EVM Mainnet" : "VEX · 6736"; color: P.color("text"); font.pixelSize: 14 }
                        Rectangle { width: 1; height: 29; color: P.color("border") }
                        ColumnLayout {
                            spacing: 2; Layout.maximumWidth: root.width > 1200 ? 280 : 170; Layout.minimumWidth: 0
                            Text { Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight; text: root.state.mining && root.state.miningHolds.length > 0 ? root.state.miningStatus : root.state.networkReadiness; color: root.state.networkHealthy && (!root.state.mining || root.state.miningHolds.length === 0) ? P.color("text") : P.color("yellow"); font.pixelSize: 13; ToolTip.visible: !!root.Window.window && (connectionHover.hovered); ToolTip.text: root.state.networkReadiness + (root.state.mining ? "\nMiner: " + root.state.miningExplanation : "\nMiner stopped"); HoverHandler { id: connectionHover } }
                            Text { text: root.state.height === "—" ? "Checking mainnet" : "Block " + root.state.height; color: P.color("faint"); font.pixelSize: 11 }
                        }
                        ActionButton {
                            objectName: "walletBadge"; text: root.state.address ? root.state.shortAddress(root.state.address) : root.state.hasWallet ? "Unlock Wallet" : "Set Up Wallet"; symbol: root.state.unlocked ? "wallet" : "lock"; compact: true
                            onClicked: {if(root.state.hasWallet&&!root.state.unlocked)root.showWallet("unlock");else{if(root.state.showIntroduction)root.state.completeIntroduction();root.state.page="Wallet"}}
                        }
                        RowLayout {
                            spacing: 2
                            Repeater {
                                model: ["minus","maximize","close"]
                                delegate: ToolButton {
                                    id: windowButton
                                    required property string modelData
                                    objectName: "window"+modelData
                                    implicitWidth: 40; implicitHeight: 40; padding: 10
                                    Accessible.name: modelData==="minus"?"Minimize":modelData==="maximize"?"Maximize or restore":"Close"
                                    onClicked: {if(modelData==="minus")root.showMinimized();else if(modelData==="maximize")root.toggleMaximize();else root.close()}
                                    contentItem: Icon { name: windowButton.modelData; tint: windowButton.hovered&&windowButton.modelData==="close" ? P.color("red") : P.color("muted") }
                                    background: Rectangle { radius: 7; color: windowButton.hovered ? "#232b32" : "transparent"; border.color: windowButton.visualFocus ? P.color("orange") : "transparent" }
                                }
                            }
                        }
                    }
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: P.color("border") }
                }
                Loader { id: pageLoader; objectName: "pageLoader"; Layout.fillWidth: true; Layout.fillHeight: true; asynchronous: false; focus: true }
                Connections { target: pageLoader.item; ignoreUnknownSignals: true; function onWalletRequested(action) {root.showWallet(action)} function onUpdateRequested() {updateDialog.open()} }
            }
        }
    }
    Repeater {
        model: [Qt.LeftEdge,Qt.RightEdge,Qt.TopEdge,Qt.BottomEdge]
        delegate: MouseArea {
            required property int modelData
            enabled: !root.expanded
            x: modelData===Qt.RightEdge ? root.width-8 : 0; y: modelData===Qt.BottomEdge ? root.height-8 : 0
            width: modelData===Qt.LeftEdge||modelData===Qt.RightEdge ? 8 : root.width
            height: modelData===Qt.TopEdge||modelData===Qt.BottomEdge ? 8 : root.height
            cursorShape: modelData===Qt.LeftEdge||modelData===Qt.RightEdge ? Qt.SizeHorCursor : Qt.SizeVerCursor
            onPressed: root.startSystemResize(modelData)
        }
    }
    WalletDialog { id: walletDialog; objectName: "walletDialog"; state: root.state }
    UpdateDialog { id: updateDialog; state: root.state }
    Dialog {
        id: recoveryDialog; objectName: "recoveryDialog"
        property string kind: ""
        property string secret: ""
        property string error: ""
        parent: Overlay.overlay; anchors.centerIn: parent; modal: true; focus: true
        width: Math.min(600,parent?parent.width-50:600); padding: 26
        closePolicy: Popup.CloseOnEscape
        onClosed: {secret="";kind="";firstWord.text="";lastWord.text="";error="";recoveryTimer.stop()}
        background: Rectangle {color:P.color("panel");radius:16;border.color:P.color("border")}
        contentItem: ColumnLayout {
            spacing:18
            Text {text:recoveryDialog.kind;color:P.color("text");font.pixelSize:25;font.bold:true;Layout.fillWidth:true}
            Text {text:"Write these details down privately and store them safely. Anyone with them can access your wallet. This view closes after 60 seconds.";color:P.color("yellow");font.pixelSize:14;wrapMode:Text.WordWrap;Layout.fillWidth:true}
            Rectangle {
                color:P.color("background");radius:10;Layout.fillWidth:true;implicitHeight:recoveryWords.implicitHeight+36
                Text {id:recoveryWords;objectName:"recoveryWords";text:recoveryDialog.secret;anchors.fill:parent;anchors.margins:18;color:P.color("text");font.pixelSize:18;font.family:"monospace";wrapMode:Text.WrapAnywhere;textFormat:Text.PlainText}
            }
            Text {visible:recoveryDialog.kind==="Recovery phrase";text:"Confirm the first and last words in order.";color:P.color("muted");font.pixelSize:13}
            RowLayout {
                visible:recoveryDialog.kind==="Recovery phrase";Layout.fillWidth:true
                TextField {id:firstWord;objectName:"recoveryFirstWord";placeholderText:"First word";Layout.fillWidth:true;Accessible.name:"First recovery word";echoMode:TextInput.Password}
                TextField {id:lastWord;objectName:"recoveryLastWord";placeholderText:"Last word";Layout.fillWidth:true;Accessible.name:"Last recovery word";echoMode:TextInput.Password}
            }
            Text {visible:recoveryDialog.error.length>0;text:recoveryDialog.error;color:P.color("red");wrapMode:Text.WordWrap;Layout.fillWidth:true}
            RowLayout {
                Layout.fillWidth:true
                ActionButton {text:"Close";onClicked:recoveryDialog.close()}
                Item {Layout.fillWidth:true}
                ActionButton {text:"I Saved My Recovery Details";primary:true;onClicked:{var words=recoveryDialog.secret.split(" ");if(recoveryDialog.kind==="Recovery phrase"&&(firstWord.text.trim()!==words[0]||lastWord.text.trim()!==words[words.length-1])){recoveryDialog.error="Check the first and last words.";return}recoveryDialog.close()}}
            }
        }
    }
    onClosing: function(event) { if(root.state.updates.installing)event.accepted=false }
    Timer {id:recoveryTimer;interval:60000;onTriggered:recoveryDialog.close()}
    FileDialog { id: backupChooser; title: "Save encrypted wallet backup"; fileMode: FileDialog.SaveFile; defaultSuffix: "json"; nameFilters: ["Encrypted wallet (*.json)"]; onAccepted: walletDialog.openAction("backup",String(selectedFile)) }
    Rectangle {
        visible: root.toast.length>0; z: 150; anchors.bottom: root.contentItem.bottom; anchors.bottomMargin: 25; anchors.horizontalCenter: root.contentItem.horizontalCenter
        width: Math.min(parent.width-60,620); height: toastText.implicitHeight+30; radius: 11; color: root.toastError ? "#3c2028" : "#163126"; border.color: root.toastError ? "#80505c" : "#3c7356"
        Text { id: toastText; anchors.centerIn: parent; width: parent.width-32; text: root.toast; color: P.color("text"); font.pixelSize: 13; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter }
        Accessible.role: Accessible.AlertMessage; Accessible.name: root.toast
    }
    Rectangle {
        objectName: "bootSplash"
        visible: opacity>0; opacity: root.booting ? 1 : 0; z: 200; anchors.fill: root.contentItem; color: P.color("background"); radius: root.expanded ? 0 : 12
        Behavior on opacity { NumberAnimation { duration: 200 } }
        Column { anchors.centerIn: parent; spacing: 20
            Image { source: "qrc:/branding/app.png"; width: 132; height: 132; fillMode: Image.PreserveAspectFit; anchors.horizontalCenter: parent.horizontalCenter }
            Text { text: "ZORU Miner"; color: P.color("text"); font.pixelSize: 31; font.weight: Font.DemiBold; anchors.horizontalCenter: parent.horizontalCenter }
        }
    }
}
