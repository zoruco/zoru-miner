import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import QtQuick.Dialogs

ApplicationWindow {
  id:root
  visible:true
  width:1230
  height:820
  minimumWidth:970
  minimumHeight:670
  title:"ZORU Miner"
  color:appBase
  property bool dark:zoru.darkMode
  property int section:0
  property color appBase:dark?"#0D1012":"#F6F7F5"
  property color surface:dark?"#15191C":"#FFFFFF"
  property color surfaceAlt:dark?"#1B2022":"#EFF1EF"
  property color edgeColor:dark?"#2D3234":"#DFE4E1"
  property color primary:dark?"#F1F3F1":"#1A211F"
  property color secondary:dark?"#929E9C":"#687572"
  property color copper:dark?"#C99170":"#A46E51"
  property color subtleCopper:dark?"#2B2421":"#F2E9E3"
  property color positive:"#6FB99A"
  property string walletMode:"create"
  property string toast:""
  property bool toastError:false
  property string importKind:"recovery_phrase"
  property string importFileUrl:""

  function addressLabel(value) {
    return value.length<22?value:value.substring(0,10)+" ··· "+value.substring(value.length-8)
  }
  function pageTitle(){return section===0?"Overview":section===1?"Wallet":"Settings"}
  function openAction(mode){
    if(zoru.busy)return
    if(mode==="import"){
      importKind="recovery_phrase"
      importFileUrl=""
      importedSecret.text=""
      importWalletPassword.text=""
      importConfirmPassword.text=""
      importDialog.open()
      return
    }
    walletMode=mode
    passInput.text=""
    confirmInput.text=""
    walletDialog.open()
    passInput.forceActiveFocus()
  }
  Connections {
    target:zoru
    function onNotification(message,isError){
      toast=message
      toastError=isError
      toastTimer.restart()
    }
  }
  Timer{id:toastTimer;interval:4500;onTriggered:toast=""}

  RowLayout {
    anchors.fill:parent
    visible:!zoru.showIntroduction
    spacing:0
    Rectangle {
      Layout.preferredWidth:206
      Layout.fillHeight:true
      color:dark?"#111517":"#ECEFEC"
      Rectangle{anchors.right:parent.right;width:1;height:parent.height;color:root.edgeColor}
      ColumnLayout{
        anchors.fill:parent
        anchors.margins:20
        spacing:0
        RowLayout {
          Layout.fillWidth:true
          spacing:10
          Image{
            source:"qrc:/assets/zoru-panda.png"
            Layout.preferredWidth:29
            Layout.preferredHeight:29
            fillMode:Image.PreserveAspectFit
          }
          Text{text:"ZORU";color:primary;font.pixelSize:19;font.weight:Font.Bold;font.letterSpacing:2}
          Item{Layout.fillWidth:true}
        }
        Text{text:"MINER";font.pixelSize:9;font.weight:Font.DemiBold;font.letterSpacing:3;color:secondary;leftPadding:39;topPadding:2}
        Item{Layout.preferredHeight:65}
        Text{
          text:"WORKSPACE"
          font.pixelSize:9
          font.weight:Font.Bold
          font.letterSpacing:1.5
          color:secondary
          leftPadding:12
        }
        Item{Layout.preferredHeight:16}
        Repeater{
          model:["Overview","Wallet","Settings"]
          delegate:Rectangle{
            Layout.fillWidth:true
            Layout.preferredHeight:45
            radius:9
            color:section===index?surfaceAlt:"transparent"
            property bool selected:section===index
            Row{
              anchors.verticalCenter:parent.verticalCenter
              leftPadding:13
              spacing:13
              Text{
                text:index===0?"▤":index===1?"▱":"◇"
                color:selected?copper:secondary
                font.pixelSize:17
                width:17
              }
              Text{
                text:modelData
                color:selected?primary:secondary
                font.pixelSize:12
                font.weight:selected?Font.DemiBold:Font.Normal
                anchors.verticalCenter:parent.verticalCenter
              }
            }
            MouseArea{anchors.fill:parent;cursorShape:Qt.PointingHandCursor;onClicked:section=index}
          }
        }
        Item{Layout.fillHeight:true}
        Rectangle {
          Layout.fillWidth:true
          Layout.preferredHeight:1
          color:root.edgeColor
        }
        Item{Layout.preferredHeight:16}
        Item{Layout.preferredHeight:9}
        Text{text:"VEX EVM  ·  MAINNET";font.pixelSize:9;color:secondary;font.letterSpacing:1.1}
      }
    }

    ColumnLayout{
      Layout.fillWidth:true
      Layout.fillHeight:true
      spacing:0
      Rectangle{
        Layout.fillWidth:true
        Layout.preferredHeight:67
        color:appBase
        Rectangle{anchors.bottom:parent.bottom;height:1;width:parent.width;color:root.edgeColor}
        RowLayout{
          anchors.fill:parent
          anchors.leftMargin:31
          anchors.rightMargin:31
          spacing:12
          Text{text:pageTitle();color:primary;font.pixelSize:13;font.weight:Font.DemiBold}
          Item{Layout.fillWidth:true}
          Rectangle {
            width:8;height:8;radius:4
            color:zoru.network==="Connected"?positive:secondary
          }
          Text{text:zoru.network;color:secondary;font.pixelSize:11}
          Rectangle{width:1;height:21;color:root.edgeColor}
          Text{text:"v"+zoru.version;color:secondary;font.pixelSize:10}
        }
      }

      Flickable {
        id:scroller
        Layout.fillWidth:true
        Layout.fillHeight:true
        contentWidth:width
        contentHeight:mainArea.implicitHeight+76
        clip:true
        ScrollBar.vertical:ScrollBar{policy:ScrollBar.AsNeeded}
        ColumnLayout{
          id:mainArea
          x:36;y:37
          width:scroller.width-72
          spacing:22

          RowLayout{
            Layout.fillWidth:true
            spacing:18
            ColumnLayout{
              spacing:6
              Text{
                text:section===0?"Mining overview":section===1?"My wallet":"Preferences"
                color:primary
                font.pixelSize:29
                font.weight:Font.Bold
              }
              Text{
                text:section===0?"Your local mining workspace.":section===1?"Secure access to your mining wallet.":"Personalize your desktop experience."
                color:secondary
                font.pixelSize:12
              }
            }
            Item{Layout.fillWidth:true}
          }
          Item{Layout.preferredHeight:1}

          RowLayout{
            visible:section===0
            Layout.fillWidth:true
            spacing:14
            Rectangle{
              Layout.fillWidth:true
              Layout.preferredHeight:239
              radius:12
              color:surface
              border.color:root.edgeColor
              ColumnLayout{
                anchors.fill:parent
                anchors.margins:23
                spacing:12
                RowLayout{
                  spacing:8
                  Rectangle{width:7;height:7;radius:4;color:secondary}
                  Text{text:"MINING ENGINE";color:secondary;font.pixelSize:10;font.letterSpacing:1.5;font.weight:Font.DemiBold}
                  Item{Layout.fillWidth:true}
                  Text{text:"CPU";color:secondary;font.pixelSize:10}
                }
                Item{Layout.preferredHeight:4}
                Text{text:zoru.miningStatus;font.pixelSize:28;color:primary;font.weight:Font.Bold}
                Text{
                  text:zoru.mining?"Mining securely from your wallet.":"Your CPU works locally; accepted proofs receive rewards on-chain."
                  font.pixelSize:12;color:secondary
                  wrapMode:Text.WordWrap
                  Layout.fillWidth:true
                }
                Item{Layout.fillHeight:true}
                RowLayout{
                  Layout.fillWidth:true
                  spacing:9
                  Text{text:"CPU threads";font.pixelSize:11;color:secondary}
                  Rectangle{
                    width:31;height:30;radius:7;color:surfaceAlt;border.color:root.edgeColor
                    Text{anchors.centerIn:parent;text:"−";color:primary;font.pixelSize:15}
                    MouseArea{
                      anchors.fill:parent
                      enabled:!zoru.mining
                      cursorShape:Qt.PointingHandCursor
                      onClicked:zoru.cpuThreads=Math.max(1,zoru.cpuThreads-1)
                    }
                  }
                  Text{text:zoru.cpuThreads+"/"+zoru.maxThreads;color:primary;font.pixelSize:11}
                  Rectangle{
                    width:31;height:30;radius:7;color:surfaceAlt;border.color:root.edgeColor
                    Text{anchors.centerIn:parent;text:"+";color:primary;font.pixelSize:15}
                    MouseArea{
                      anchors.fill:parent
                      enabled:!zoru.mining
                      cursorShape:Qt.PointingHandCursor
                      onClicked:zoru.cpuThreads=Math.min(zoru.maxThreads,zoru.cpuThreads+1)
                    }
                  }
                  Item{Layout.fillWidth:true}
                  Rectangle{
                    implicitWidth:124;implicitHeight:38;radius:8
                    color:zoru.mining?surfaceAlt:copper
                    opacity:zoru.minerAvailable && zoru.network==="Connected"?1:0.5
                    Text{
                      anchors.centerIn:parent
                      text:zoru.mining?"Stop mining":"Start mining"
                      font.pixelSize:12;font.weight:Font.DemiBold
                      color:zoru.mining?primary:"#161412"
                    }
                    MouseArea{
                      anchors.fill:parent
                      cursorShape:Qt.PointingHandCursor
                      onClicked:{
                        if(zoru.mining)zoru.stopMining()
                        else if(!zoru.hasWallet){section=1;openAction("create")}
                        else if(!zoru.minerAvailable){toast="Mining engine is not installed";toastError=true;toastTimer.restart()}
                        else if(zoru.network!=="Connected"){toast="Network unavailable";toastError=true;toastTimer.restart()}
                        else openAction("mine")
                      }
                    }
                  }
                }
              }
            }
            Rectangle{
              Layout.preferredWidth:278
              Layout.preferredHeight:239
              radius:12;color:surface;border.color:root.edgeColor
              ColumnLayout{
                anchors.fill:parent
                anchors.margins:22
                spacing:13
                RowLayout{
                  Text{text:"MINING WALLET";color:secondary;font.pixelSize:10;font.letterSpacing:1.5;font.weight:Font.DemiBold}
                  Item{Layout.fillWidth:true}
                  Text{text:"↗";font.pixelSize:17;color:secondary}
                }
                Text{
                  text:zoru.unlocked?addressLabel(zoru.address):zoru.hasWallet?"Wallet locked":"No wallet connected"
                  color:primary;font.pixelSize:zoru.unlocked?15:17;font.weight:Font.DemiBold
                }
                Text{
                  text:zoru.unlocked?zoru.balance+" VEX":zoru.hasWallet?"Unlock to view your balance":"Your keys remain on this device"
                  color:secondary;font.pixelSize:11
                }
                Item{Layout.fillHeight:true}
                Rectangle{
                  Layout.fillWidth:true
                  height:37;radius:8;color:subtleCopper
                  Text{
                    anchors.centerIn:parent
                    text:zoru.hasWallet?"Manage wallet  →":"Create wallet  →"
                    font.pixelSize:11;font.weight:Font.DemiBold;color:copper
                  }
                  MouseArea{
                    anchors.fill:parent;cursorShape:Qt.PointingHandCursor
                    onClicked:zoru.hasWallet?section=1:openAction("create")
                  }
                }
              }
            }
          }

          RowLayout{
            visible:section===0
            Layout.fillWidth:true
            spacing:14
            Repeater{
              model:[
                {name:"Hashrate",number:zoru.hashrate,unit:"H/s",note:zoru.mining?"Current CPU rate":"No active session"},
                {name:"Proofs found",number:zoru.proofsFound.toString(),unit:"",note:"Pending proof acceptance"},
                {name:"Accepted proofs",number:zoru.rewardsConfirmed.toString(),unit:"",note:"On-chain accepted proofs this session"}
              ]
              delegate:Rectangle{
                Layout.fillWidth:true
                Layout.preferredHeight:130
                radius:12;color:surface;border.color:root.edgeColor
                Column{
                  anchors.fill:parent;anchors.margins:19;spacing:13
                  Text{text:modelData.name;color:secondary;font.pixelSize:11}
                  Row{
                    spacing:8
                    Text{text:modelData.number;color:primary;font.pixelSize:27;font.weight:Font.Bold}
                    Text{text:modelData.unit;color:secondary;font.pixelSize:11;anchors.bottom:parent.bottom;bottomPadding:3}
                  }
                  Text{text:modelData.note;color:secondary;font.pixelSize:10}
                }
              }
            }
          }

          RowLayout{
            visible:section===0
            Layout.fillWidth:true
            spacing:14
            Rectangle{
              Layout.fillWidth:true
              Layout.preferredHeight:248
              radius:12;color:surface;border.color:root.edgeColor
              ColumnLayout{
                anchors.fill:parent
                anchors.margins:20
                spacing:8
                RowLayout{
                  Text{text:"Hashrate history";color:primary;font.pixelSize:14;font.weight:Font.DemiBold}
                  Item{Layout.fillWidth:true}
                  Text{text:"THIS SESSION";color:secondary;font.pixelSize:9;font.letterSpacing:1}
                }
                Item {
                  Layout.fillWidth:true
                  Layout.fillHeight:true
                  Repeater{
                    model:4
                    Rectangle{
                      x:0
                      y:(index+1)*parent.height/5
                      width:parent.width;height:1
                      color:root.edgeColor
                      opacity:0.5
                    }
                  }
                  Canvas {
                    id:rateCanvas
                    anchors.fill:parent
                    anchors.margins:7
                    visible:zoru.hashrateSamples.length>1
                    onPaint:{
                      const ctx=getContext("2d")
                      ctx.clearRect(0,0,width,height)
                      const arr=zoru.hashrateSamples
                      if(arr.length<2)return
                      let max=1
                      for(let j=0;j<arr.length;j++)max=Math.max(max,Number(arr[j]))
                      ctx.strokeStyle=root.copper
                      ctx.lineWidth=2
                      ctx.lineJoin="round"
                      ctx.lineCap="round"
                      ctx.beginPath()
                      for(let i=0;i<arr.length;i++){
                        const x=i*width/(arr.length-1)
                        const y=height-10-(arr[i]/max)*(height-20)
                        if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y)
                      }
                      ctx.stroke()
                    }
                    Connections{
                      target:zoru
                      function onStatsChanged(){rateCanvas.requestPaint()}
                    }
                  }
                  Column{
                    visible:zoru.hashrateSamples.length<2
                    anchors.centerIn:parent
                    spacing:8
                    Text{text:"No performance data yet";color:primary;font.pixelSize:12;font.weight:Font.DemiBold;anchors.horizontalCenter:parent.horizontalCenter}
                    Text{text:"The chart starts when mining reports a hashrate.";color:secondary;font.pixelSize:11;anchors.horizontalCenter:parent.horizontalCenter}
                  }
                }
              }
            }
            Rectangle {
              Layout.preferredWidth:278
              Layout.preferredHeight:248
              radius:12;color:surface;border.color:root.edgeColor
              ColumnLayout{
                anchors.fill:parent
                anchors.margins:20
                spacing:16
                Text{text:"Activity";color:primary;font.pixelSize:14;font.weight:Font.DemiBold}
                Rectangle{Layout.fillWidth:true;height:1;color:root.edgeColor}
                Item{Layout.fillHeight:true}
                Text{text:zoru.lastMiningEvent;color:primary;font.pixelSize:12;font.weight:Font.DemiBold;Layout.fillWidth:true;horizontalAlignment:Text.AlignHCenter}
                Text{
                  text:zoru.mining?"Mining activity is updated as events arrive.":"Start mining to see session activity."
                  color:secondary;font.pixelSize:11;wrapMode:Text.WordWrap
                  horizontalAlignment:Text.AlignHCenter
                  Layout.fillWidth:true
                }
                Item{Layout.fillHeight:true}
              }
            }
          }

          Rectangle {
            visible:section===1
            Layout.fillWidth:true
            Layout.preferredHeight:zoru.hasWallet?280:270
            color:surface;radius:12;border.color:root.edgeColor
            ColumnLayout{
              anchors.fill:parent;anchors.margins:28;spacing:13
              RowLayout {
                Text{text:"WALLET ADDRESS";color:secondary;font.pixelSize:10;font.weight:Font.DemiBold;font.letterSpacing:1.3}
                Item{Layout.fillWidth:true}
                Rectangle{
                  implicitWidth:82;implicitHeight:27;radius:14
                  color:surfaceAlt
                  Text{
                    anchors.centerIn:parent
                    text:zoru.unlocked?"Unlocked":zoru.hasWallet?"Locked":"Not set up"
                    color:secondary;font.pixelSize:10
                  }
                }
              }
              Item{Layout.preferredHeight:10}
              Text{
                text:zoru.unlocked?addressLabel(zoru.address):zoru.hasWallet?"Wallet locked":"No wallet connected"
                color:primary;font.pixelSize:24;font.weight:Font.Bold
              }
              Text{
                text:zoru.unlocked?zoru.balance+" VEX":zoru.hasWallet?"Unlock to view your VEX balance.":"Create a new wallet or import an encrypted wallet."
                color:secondary;font.pixelSize:13
              }
              Item{Layout.fillHeight:true}
              RowLayout{
                spacing:10
                Repeater{
                  model:zoru.hasWallet?(zoru.unlocked?["Copy address","Claim rewards","Lock","Back up"]:["Unlock","Back up"]):["Create wallet","Import wallet"]
                  delegate:Rectangle{
                    implicitWidth:Math.max(115,buttonLabel.implicitWidth+29)
                    implicitHeight:39
                    radius:8
                    color:index===0?copper:surfaceAlt
                    border.color:index===0?"transparent":root.edgeColor
                    Text{id:buttonLabel;anchors.centerIn:parent;text:modelData;color:index===0?"#141414":primary;font.pixelSize:11;font.weight:Font.DemiBold}
                    MouseArea{
                      anchors.fill:parent;cursorShape:Qt.PointingHandCursor
                      onClicked:{
                        if(modelData==="Create wallet")openAction("create")
                        else if(modelData==="Import wallet")openAction("import")
                        else if(modelData==="Unlock")openAction("unlock")
                        else if(modelData==="Copy address")zoru.copyAddress()
                        else if(modelData==="Claim rewards")openAction("claim")
                        else if(modelData==="Lock")zoru.lockWallet()
                        else if(modelData==="Back up")backupChooser.open()
                      }
                    }
                  }
                }
                Item{Layout.fillWidth:true}
              }
            }
          }
          Rectangle{
            visible:section===1
            Layout.fillWidth:true;Layout.preferredHeight:101
            radius:12;color:surface;border.color:root.edgeColor
            RowLayout{
              anchors.fill:parent;anchors.margins:23;spacing:15
              Rectangle{width:36;height:36;radius:8;color:surfaceAlt
                Text{anchors.centerIn:parent;text:"◇";color:secondary;font.pixelSize:20}
              }
              ColumnLayout{
                Layout.fillWidth:true;spacing:5
                Text{text:"Your wallet stays on this device";color:primary;font.pixelSize:12;font.weight:Font.DemiBold}
                Text{text:"Keep your encrypted backup and password somewhere safe. Password recovery is not available.";color:secondary;font.pixelSize:11;wrapMode:Text.WordWrap;Layout.fillWidth:true}
              }
            }
          }
          Rectangle{
            visible:section===2
            Layout.fillWidth:true;Layout.preferredHeight:176
            radius:12;color:surface;border.color:root.edgeColor
            ColumnLayout{
              anchors.fill:parent;anchors.margins:23;spacing:17
              Text{text:"Appearance";color:primary;font.pixelSize:14;font.weight:Font.DemiBold}
              Rectangle{Layout.fillWidth:true;height:1;color:root.edgeColor}
              RowLayout{
                ColumnLayout{
                  spacing:5
                  Text{text:"Theme";color:primary;font.pixelSize:12;font.weight:Font.DemiBold}
                  Text{text:"Use a light or dark workspace.";color:secondary;font.pixelSize:11}
                }
                Item{Layout.fillWidth:true}
                Rectangle{
                  width:46;height:26;radius:13
                  color:zoru.darkMode?copper:surfaceAlt
                  border.color:zoru.darkMode?copper:root.edgeColor
                  Rectangle{
                    x:zoru.darkMode?23:3
                    y:3;width:20;height:20;radius:10
                    color:zoru.darkMode?"#171514":primary
                    Behavior on x{NumberAnimation{duration:155;easing.type:Easing.OutCubic}}
                  }
                  MouseArea{
                    anchors.fill:parent;cursorShape:Qt.PointingHandCursor
                    onClicked:zoru.darkMode=!zoru.darkMode
                  }
                }
                Text{text:dark?"Dark":"Light";color:secondary;font.pixelSize:11}
              }
            }
          }
          Rectangle{
            visible:section===2
            Layout.fillWidth:true
            Layout.preferredHeight:125
            radius:12;color:surface;border.color:root.edgeColor
            RowLayout{
              anchors.fill:parent;anchors.margins:23;spacing:15
              ColumnLayout{
                Layout.fillWidth:true;spacing:7
                Text{text:"Reset preferences";color:primary;font.pixelSize:14;font.weight:Font.DemiBold}
                Text{
                  text:"Restore the default appearance and CPU settings. Your wallet stays untouched."
                  color:secondary;font.pixelSize:11
                  wrapMode:Text.WordWrap;Layout.fillWidth:true
                }
              }
              Rectangle{
                width:128;height:37;radius:8;color:surfaceAlt;border.color:root.edgeColor
                Text{anchors.centerIn:parent;text:"Reset settings";font.pixelSize:11;font.weight:Font.DemiBold;color:primary}
                MouseArea{anchors.fill:parent;cursorShape:Qt.PointingHandCursor;onClicked:zoru.resetSettings()}
              }
            }
          }
          Rectangle{
            visible:section===2 && zoru.hasWallet
            Layout.fillWidth:true
            Layout.preferredHeight:136
            radius:12;color:surface;border.color:root.edgeColor
            RowLayout{
              anchors.fill:parent;anchors.margins:23;spacing:15
              ColumnLayout{
                Layout.fillWidth:true;spacing:7
                Text{text:"Remove local wallet";color:primary;font.pixelSize:14;font.weight:Font.DemiBold}
                Text{
                  text:"Removes the encrypted wallet from this device. Keep a backup before proceeding."
                  color:secondary;font.pixelSize:11
                  wrapMode:Text.WordWrap;Layout.fillWidth:true
                }
              }
              Rectangle{
                width:135;height:37;radius:8;color:dark?"#291D1D":"#FAEEEE"
                border.color:dark?"#68413D":"#E5C5C2"
                Text{anchors.centerIn:parent;text:"Remove wallet";font.pixelSize:11;font.weight:Font.DemiBold;color:dark?"#E5A49B":"#9F514A"}
                MouseArea{
                  anchors.fill:parent;cursorShape:Qt.PointingHandCursor
                  onClicked:removeDialog.open()
                }
              }
            }
          }
          Item{Layout.preferredHeight:9}
          RowLayout{
            Layout.fillWidth:true
            Text{text:"ZORU Miner";color:secondary;font.pixelSize:10}
            Item{Layout.fillWidth:true}
            Text{text:"VEX EVM";color:secondary;font.pixelSize:10}
          }
        }
      }
    }
  }

  Rectangle {
    id:introduction
    visible:zoru.showIntroduction
    anchors.fill:parent
    z:90
    color:appBase
    ColumnLayout {
      anchors.centerIn:parent
      width:Math.min(parent.width-80,650)
      spacing:13
      Image {
        Layout.alignment:Qt.AlignHCenter
        Layout.preferredWidth:132
        Layout.preferredHeight:132
        source:"qrc:/assets/zoru-panda.png"
        fillMode:Image.PreserveAspectFit
        smooth:true
      }
      Item{Layout.preferredHeight:12}
      Text {
        text:"Welcome to ZORU Miner"
        Layout.alignment:Qt.AlignHCenter
        color:primary
        font.pixelSize:30
        font.weight:Font.Bold
      }
      Text {
        text:"Your wallet and mining workspace, all in one place."
        Layout.alignment:Qt.AlignHCenter
        horizontalAlignment:Text.AlignHCenter
        color:secondary
        font.pixelSize:13
      }
      Item{Layout.preferredHeight:21}
      RowLayout {
        Layout.alignment:Qt.AlignHCenter
        spacing:18
        Text{text:"Encrypted wallet";color:secondary;font.pixelSize:11}
        Rectangle{width:3;height:3;radius:2;color:copper}
        Text{text:"CPU mining";color:secondary;font.pixelSize:11}
        Rectangle{width:3;height:3;radius:2;color:copper}
        Text{text:"Your own keys";color:secondary;font.pixelSize:11}
      }
      Item{Layout.preferredHeight:27}
      Rectangle {
        Layout.alignment:Qt.AlignHCenter
        width:242;height:46
        radius:9;color:copper
        Text{anchors.centerIn:parent;text:"Create mining wallet  →";font.pixelSize:13;font.weight:Font.DemiBold;color:"#171412"}
        MouseArea {
          anchors.fill:parent
          cursorShape:Qt.PointingHandCursor
          onClicked:{zoru.completeIntroduction();openAction("create")}
        }
      }
      Text {
        text:"Import existing wallet"
        Layout.alignment:Qt.AlignHCenter
        color:secondary
        font.pixelSize:12
        font.underline:true
        MouseArea {
          anchors.fill:parent
          cursorShape:Qt.PointingHandCursor
          onClicked:{zoru.completeIntroduction();openAction("import")}
        }
      }
      Item{Layout.preferredHeight:9}
      Text{
        text:"Explore without a wallet"
        Layout.alignment:Qt.AlignHCenter
        color:secondary
        font.pixelSize:11
        MouseArea {anchors.fill:parent;cursorShape:Qt.PointingHandCursor;onClicked:zoru.completeIntroduction()}
      }
    }
  }

  Dialog {
    id:walletDialog
    width:460
    height:walletMode==="create"?360:walletMode==="mine"?344:302
    anchors.centerIn:Overlay.overlay
    modal:true
    focus:true
    padding:26
    property string inlineError:""
    onOpened:{
      inlineError=""
      passInput.passwordVisible=false
      confirmInput.passwordVisible=false
      passInput.forceActiveFocus()
    }
    closePolicy:zoru.busy?Popup.NoAutoClose:Popup.CloseOnEscape
    background:Rectangle{color:surface;radius:16;border.color:root.edgeColor}
    contentItem:ColumnLayout {
      spacing:12
      Text {text:walletMode==="create"?"Create a wallet":walletMode==="mine"?"Start mainnet mining":walletMode==="claim"?"Claim ZORU rewards":"Unlock wallet";color:primary;font.pixelSize:22;font.weight:Font.Bold}
      Text{text:walletMode==="create"?"Your wallet is encrypted and saved on this device.":walletMode==="mine"?"Mining sends signed proofs on-chain and consumes VEX gas. Enter your password to start.":walletMode==="claim"?"Claim eligible on-chain rewards after their epoch closes. This sends signed transactions and consumes VEX gas.":"Enter your wallet password to continue.";color:secondary;font.pixelSize:12;wrapMode:Text.WordWrap;Layout.fillWidth:true}
      Item{Layout.preferredHeight:6}
      Text{text:"Wallet password";color:primary;font.pixelSize:11;font.weight:Font.DemiBold}
      TextField {
        id:passInput
        property bool passwordVisible:false
        Layout.fillWidth:true
        Layout.preferredHeight:43
        echoMode:passwordVisible?TextInput.Normal:TextInput.Password
        rightPadding:49
        placeholderText:walletMode==="create"?"At least 12 characters":"Enter password"
        font.pixelSize:12
        color:primary
        placeholderTextColor:secondary
        background:Rectangle{radius:8;color:surfaceAlt;border.color:root.edgeColor}
        onAccepted:walletDialog.submitWallet()
        onTextChanged:walletDialog.inlineError=""
        Image{
          anchors.right:parent.right
          anchors.rightMargin:13
          anchors.verticalCenter:parent.verticalCenter
          width:21;height:21
          source:passInput.passwordVisible?"qrc:/assets/eye-off.png":"qrc:/assets/eye.png"
          fillMode:Image.PreserveAspectFit
          MouseArea{
            anchors.fill:parent
            cursorShape:Qt.PointingHandCursor
            onClicked:{
              passInput.passwordVisible=!passInput.passwordVisible
              passInput.forceActiveFocus()
            }
          }
        }
      }
      Text {visible:walletMode==="create";text:"Confirm password";color:primary;font.pixelSize:11;font.weight:Font.DemiBold}
      TextField{
        id:confirmInput
        property bool passwordVisible:false
        visible:walletMode==="create"
        Layout.fillWidth:true
        Layout.preferredHeight:43
        echoMode:passwordVisible?TextInput.Normal:TextInput.Password
        rightPadding:49
        placeholderText:"Repeat password"
        font.pixelSize:12
        color:primary
        placeholderTextColor:secondary
        background:Rectangle{radius:8;color:surfaceAlt;border.color:root.edgeColor}
        onAccepted:walletDialog.submitWallet()
        Image{
          anchors.right:parent.right
          anchors.rightMargin:13
          anchors.verticalCenter:parent.verticalCenter
          width:21;height:21
          source:confirmInput.passwordVisible?"qrc:/assets/eye-off.png":"qrc:/assets/eye.png"
          fillMode:Image.PreserveAspectFit
          MouseArea{
            anchors.fill:parent
            cursorShape:Qt.PointingHandCursor
            onClicked:{
              confirmInput.passwordVisible=!confirmInput.passwordVisible
              confirmInput.forceActiveFocus()
            }
          }
        }
      }
      Text{
        visible:walletDialog.inlineError.length>0
        text:walletDialog.inlineError
        color:"#E8A099"
        font.pixelSize:11
        wrapMode:Text.WordWrap
        Layout.fillWidth:true
      }
      Item{Layout.fillHeight:true}
      RowLayout{
        Layout.fillWidth:true
        spacing:12
        Item{Layout.fillWidth:true}
        Button{
          text:"Cancel"
          Layout.preferredWidth:88
          Layout.preferredHeight:42
          enabled:!zoru.busy
          onClicked:walletDialog.close()
          background:Rectangle{radius:8;color:surfaceAlt}
          contentItem:Text{text:parent.text;color:primary;font.pixelSize:12;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
        }
        Button{
          text:zoru.busy?"Working…":"Continue  →"
          Layout.preferredWidth:134
          Layout.preferredHeight:42
          enabled:!zoru.busy
          onClicked:walletDialog.submitWallet()
          background:Rectangle{radius:8;color:copper}
          contentItem:Text{text:parent.text;color:"#171412";font.pixelSize:12;font.weight:Font.Bold;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
        }
      }
    }
    function submitWallet() {
      if(zoru.busy)return
      if(passInput.text.length===0) {
        inlineError="Enter your wallet password."
        passInput.forceActiveFocus()
        return
      }
      if(walletMode==="create"&&(passInput.text.length<12||passInput.text!==confirmInput.text)){
        inlineError="Use at least 12 characters and matching passwords."
        return
      }
      const submittedPassword=passInput.text
      if(walletMode==="create")zoru.createWallet(submittedPassword)
      else if(walletMode==="unlock")zoru.unlockWallet(submittedPassword)
      else if(walletMode==="claim"){
        if(!zoru.claimRewards(submittedPassword)){
          inlineError="Claim could not start. Check wallet and network status."
          return
        }
      }
      else if(walletMode==="mine"){
        if(!zoru.startMining(submittedPassword)){
          inlineError="Mining could not start. Check your wallet and network status."
          return
        }
      }
      passInput.text=""
      confirmInput.text=""
      walletDialog.close()
    }
  }
  Dialog {
    id:importDialog
    width:510
    height:530
    anchors.centerIn:Overlay.overlay
    modal:true
    focus:true
    padding:25
    closePolicy:zoru.busy?Popup.NoAutoClose:Popup.CloseOnEscape
    background:Rectangle{color:surface;radius:16;border.color:root.edgeColor}
    onClosed:{
      importedSecret.text=""
      importWalletPassword.text=""
      importConfirmPassword.text=""
      importFileUrl=""
    }
    contentItem:ColumnLayout {
      spacing:12
      Text{text:"Import wallet";font.pixelSize:23;font.weight:Font.Bold;color:primary}
      Text{
        text:"Restore your existing EVM address. Only one wallet can be active on this device."
        font.pixelSize:12;color:secondary;wrapMode:Text.WordWrap;Layout.fillWidth:true
      }
      Item{Layout.preferredHeight:4}
      RowLayout{
        Layout.fillWidth:true
        spacing:7
        Repeater{
          model:[
            {name:"Recovery phrase",id:"recovery_phrase"},
            {name:"Private key",id:"private_key"},
            {name:"Keystore JSON",id:"keystore"}
          ]
          delegate:Rectangle{
            Layout.fillWidth:true
            Layout.preferredHeight:37
            radius:8
            color:root.importKind===modelData.id?subtleCopper:surfaceAlt
            border.color:root.importKind===modelData.id?copper:root.edgeColor
            Text{
              anchors.centerIn:parent
              text:modelData.name
              color:root.importKind===modelData.id?copper:primary
              font.pixelSize:11;font.weight:Font.DemiBold
            }
            MouseArea{
              anchors.fill:parent;cursorShape:Qt.PointingHandCursor
              onClicked:{
                root.importKind=modelData.id
                importedSecret.text=""
                importWalletPassword.text=""
                importConfirmPassword.text=""
                importFileUrl=""
              }
            }
          }
        }
      }
      Item{Layout.preferredHeight:5}
      Text{
        text:root.importKind==="recovery_phrase"?"Recovery phrase":root.importKind==="private_key"?"Private key":"Encrypted keystore file"
        font.pixelSize:11;font.weight:Font.DemiBold;color:primary
      }
      Rectangle{
        visible:root.importKind==="keystore"
        Layout.fillWidth:true;Layout.preferredHeight:49
        color:surfaceAlt;radius:8;border.color:root.edgeColor
        Text{
          anchors.verticalCenter:parent.verticalCenter
          anchors.left:parent.left;anchors.leftMargin:13
          text:root.importFileUrl.length?root.importFileUrl.split("/").pop():"Choose a .json wallet file"
          width:parent.width-140
          elide:Text.ElideMiddle;font.pixelSize:11;color:primary
        }
        Text{anchors.verticalCenter:parent.verticalCenter;anchors.right:parent.right;anchors.rightMargin:15;text:"Browse  →";color:copper;font.pixelSize:11;font.weight:Font.DemiBold}
        MouseArea{anchors.fill:parent;cursorShape:Qt.PointingHandCursor;onClicked:importChooser.open()}
      }
      TextArea {
        id:importedSecret
        visible:root.importKind!=="keystore"
        Layout.fillWidth:true
        Layout.preferredHeight:root.importKind==="recovery_phrase"?85:48
        color:primary
        placeholderText:root.importKind==="recovery_phrase"?"Enter 12 or 24 words separated by spaces":"Paste your EVM private key"
        placeholderTextColor:secondary
        font.pixelSize:12
        wrapMode:TextEdit.Wrap
        background:Rectangle{color:surfaceAlt;radius:8;border.color:root.edgeColor}
      }
      Text{
        text:root.importKind==="keystore"?"Existing keystore password":"Create a local wallet password"
        font.pixelSize:11;font.weight:Font.DemiBold;color:primary
      }
      TextField{
        id:importWalletPassword
        Layout.fillWidth:true;Layout.preferredHeight:41
        echoMode:TextInput.Password
        placeholderText:root.importKind==="keystore"?"Password that originally encrypted this file":"At least 12 characters"
        color:primary;placeholderTextColor:secondary;font.pixelSize:12
        background:Rectangle{color:surfaceAlt;radius:8;border.color:root.edgeColor}
        onAccepted:importDialog.submit()
      }
      Text{
        visible:root.importKind!=="keystore"
        text:"Confirm local password"
        font.pixelSize:11;font.weight:Font.DemiBold;color:primary
      }
      TextField{
        id:importConfirmPassword
        visible:root.importKind!=="keystore"
        Layout.fillWidth:true;Layout.preferredHeight:41
        echoMode:TextInput.Password
        placeholderText:"Repeat password"
        color:primary;placeholderTextColor:secondary;font.pixelSize:12
        background:Rectangle{color:surfaceAlt;radius:8;border.color:root.edgeColor}
        onAccepted:importDialog.submit()
      }
      Text{
        text:root.importKind==="keystore"?"The original file password is required. It will remain the wallet password.":"Your secret is encrypted locally. Never share it with anyone."
        font.pixelSize:10;color:secondary;wrapMode:Text.WordWrap;Layout.fillWidth:true
      }
      Item{Layout.fillHeight:true}
      RowLayout{
        Layout.fillWidth:true
        Item{Layout.fillWidth:true}
        Rectangle{
          width:93;height:39;radius:8;color:surfaceAlt
          Text{anchors.centerIn:parent;text:"Cancel";font.pixelSize:12;color:primary}
          MouseArea{anchors.fill:parent;cursorShape:Qt.PointingHandCursor;onClicked:importDialog.close()}
        }
        Rectangle{
          width:136;height:39;radius:8;color:copper
          opacity:zoru.busy?0.5:1
          Text{anchors.centerIn:parent;text:zoru.busy?"Importing…":"Import wallet  →";font.pixelSize:12;color:"#161412";font.weight:Font.DemiBold}
          MouseArea{anchors.fill:parent;cursorShape:Qt.PointingHandCursor;enabled:!zoru.busy;onClicked:importDialog.submit()}
        }
      }
    }
    function submit(){
      if(zoru.busy)return
      if(zoru.hasWallet){toast="A wallet already exists. Remove it before importing another.";toastError=true;toastTimer.restart();return}
      if(root.importKind==="keystore"){
        if(!root.importFileUrl.length || !importWalletPassword.text.length){
          toast="Choose your encrypted JSON file and enter its original password."
          toastError=true;toastTimer.restart();return
        }
        zoru.importWallet(root.importFileUrl,importWalletPassword.text)
      }else{
        if(!importedSecret.text.trim().length || importWalletPassword.text.length<12 || importWalletPassword.text!==importConfirmPassword.text){
          toast="Enter a valid wallet secret and matching local passwords (12+ characters)."
          toastError=true;toastTimer.restart();return
        }
        zoru.importRecovery(root.importKind,importedSecret.text.trim(),importWalletPassword.text)
      }
      importDialog.close()
    }
  }
  FileDialog{
    id:importChooser
    title:"Select encrypted EVM keystore"
    nameFilters:["JSON wallet (*.json)"]
    fileMode:FileDialog.OpenFile
    onAccepted:root.importFileUrl=selectedFile.toString()
  }
  Dialog{
    id:removeDialog
    width:455
    height:337
    anchors.centerIn:Overlay.overlay
    modal:true
    focus:true
    padding:25
    background:Rectangle{color:surface;radius:16;border.color:root.edgeColor}
    onClosed:{removePhrase.text="";backupAcknowledged.checked=false}
    contentItem:ColumnLayout{
      spacing:16
      Text{text:"Remove local wallet";color:primary;font.pixelSize:22;font.weight:Font.Bold}
      Text{
        text:"This permanently removes the encrypted wallet stored on this device. It does not remove funds from the blockchain. You cannot recover access without your original key, phrase, or backup."
        color:secondary;font.pixelSize:12;wrapMode:Text.WordWrap;Layout.fillWidth:true
      }
      CheckBox{
        id:backupAcknowledged
        text:"I have saved a recovery phrase, private key, or wallet backup."
        checked:false
        indicator:Rectangle{
          implicitWidth:19;implicitHeight:19;radius:5
          color:backupAcknowledged.checked?copper:surfaceAlt
          border.color:backupAcknowledged.checked?copper:root.edgeColor
          Text{anchors.centerIn:parent;text:backupAcknowledged.checked?"✓":"";color:"#151514";font.pixelSize:13;font.weight:Font.Bold}
        }
        contentItem:Text{
          text:backupAcknowledged.text
          color:primary;font.pixelSize:11
          wrapMode:Text.WordWrap
          verticalAlignment:Text.AlignVCenter
          leftPadding:backupAcknowledged.indicator.width+backupAcknowledged.spacing
        }
      }
      Text{text:'Type REMOVE to confirm';font.pixelSize:11;color:primary;font.weight:Font.DemiBold}
      TextField{
        id:removePhrase
        Layout.fillWidth:true;Layout.preferredHeight:39
        placeholderText:"REMOVE"
        color:primary;placeholderTextColor:secondary
        background:Rectangle{color:surfaceAlt;radius:8;border.color:root.edgeColor}
      }
      Item{Layout.fillHeight:true}
      RowLayout{
        Layout.fillWidth:true
        Item{Layout.fillWidth:true}
        Button{text:"Cancel";onClicked:removeDialog.close()}
        Button{
          text:"Remove wallet"
          enabled:backupAcknowledged.checked && removePhrase.text==="REMOVE" && !zoru.mining && !zoru.busy
          onClicked:{zoru.removeWallet("REMOVE WALLET");removeDialog.close()}
        }
      }
    }
  }
  FileDialog{
    id:backupChooser
    title:"Save encrypted wallet"
    fileMode:FileDialog.SaveFile
    defaultSuffix:"json"
    nameFilters:["Encrypted wallet (*.json)"]
    onAccepted:zoru.backupWallet(selectedFile.toString())
  }
  Rectangle {
    visible:toast!==""
    z:200
    anchors.bottom:parent.bottom
    anchors.bottomMargin:25
    anchors.horizontalCenter:parent.horizontalCenter
    width:Math.min(parent.width-50,550)
    height:50
    color:toastError?"#4a2d2c":"#304139"
    radius:10
    border.color:toastError?"#9b5851":"#517e65"
    Text {anchors.centerIn:parent;text:toast;color:"#f6f6f3";font.pixelSize:12;font.weight:Font.Medium}
  }
}
