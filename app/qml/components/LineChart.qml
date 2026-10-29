import QtQuick
import "../Palette.js" as P
Item {
    id: root
    property var samples: []
    property string unit: "H/s"
    property string emptyText: "Your hashrate appears when mining begins."
    property real scale: 1
    readonly property bool hasSamples: samples && samples.length > 0
    implicitHeight: Math.max(220, emptyMessage.implicitHeight)
    clip: true
    onSamplesChanged: chart.requestPaint()
    onScaleChanged: chart.requestPaint()
    Canvas {
        id: chart
        objectName: "chartPlot"
        visible: root.hasSamples
        anchors.fill: parent
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            var c = getContext("2d"); c.reset()
            var left = 45, top = 21, right = width - 12, bottom = height - 28
            var usable = Math.max(1,right-left), high = Math.max(1,bottom-top)
            c.strokeStyle = "#222b32"; c.lineWidth = 1
            for(var gy=0;gy<=4;gy++){var y=top+high*gy/4;c.beginPath();c.moveTo(left,y);c.lineTo(right,y);c.stroke()}
            for(var gx=0;gx<=8;gx++){var x=left+usable*gx/8;c.beginPath();c.moveTo(x,top);c.lineTo(x,bottom);c.stroke()}
            c.fillStyle=P.color("faint");c.font="11px sans-serif";c.fillText(root.unit,left+6,top-6)
            if(!root.samples || root.samples.length===0)return
            var values=[],maximum=0
            for(var i=0;i<root.samples.length;i++){var v=Number(root.samples[i].value)/root.scale;if(!isFinite(v)||v<0)return;values.push(v);maximum=Math.max(maximum,v)}
            maximum=Math.max(maximum*1.15,1)
            for(var k=0;k<=4;k++){var tick=maximum*(1-k/4);c.fillText(tick>=1000?(tick/1000).toFixed(1)+"k":tick.toFixed(tick<10?1:0),3,top+high*k/4+4)}
            var points=[]
            for(var p=0;p<values.length;p++){points.push({x:values.length===1?left+usable/2:left+usable*p/(values.length-1),y:bottom-high*values[p]/maximum})}
            if(points.length>1){
                var gradient=c.createLinearGradient(0,top,0,bottom);gradient.addColorStop(0,"#71371b");gradient.addColorStop(1,"#10161a")
                c.beginPath();c.moveTo(points[0].x,bottom);for(var j=0;j<points.length;j++)c.lineTo(points[j].x,points[j].y);c.lineTo(points[points.length-1].x,bottom);c.closePath();c.fillStyle=gradient;c.fill()
                c.beginPath();c.moveTo(points[0].x,points[0].y);for(var n=1;n<points.length;n++)c.lineTo(points[n].x,points[n].y);c.strokeStyle="#ff862c";c.lineWidth=2.3;c.shadowColor="#ff7b26";c.shadowBlur=7;c.stroke();c.shadowBlur=0;c.shadowColor="transparent"
            }
            var last=points[points.length-1];c.beginPath();c.arc(last.x,last.y,4,0,Math.PI*2);c.fillStyle=P.color("orange");c.fill()
            c.fillStyle=P.color("muted");c.font="11px sans-serif"
            if(root.samples[0].timestamp)c.fillText(Qt.formatTime(new Date(root.samples[0].timestamp*1000),"HH:mm"),left,bottom+21)
            if(root.samples.length>1&&root.samples[root.samples.length-1].timestamp)c.fillText(Qt.formatTime(new Date(root.samples[root.samples.length-1].timestamp*1000),"HH:mm"),Math.max(left,right-35),bottom+21)
        }
    }
    EmptyState {
        id: emptyMessage
        objectName: "chartEmpty"
        visible: !root.hasSamples
        anchors.fill: parent
        title: root.unit === "ZORU" ? "No earnings yet" : root.unit === "ms" ? "No measurements yet" : "No hashrate yet"
        description: root.emptyText
        symbol: root.unit === "ZORU" ? "rewards" : root.unit === "ms" ? "nodes" : "miner"
    }
}
