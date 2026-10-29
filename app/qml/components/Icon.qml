import QtQuick
Canvas {
    id: root
    property string name: "overview"
    property color tint: "#a4aeba"
    implicitWidth: 24
    implicitHeight: 24
    onNameChanged: requestPaint()
    onTintChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
        var c = getContext("2d"); c.reset(); c.scale(width / 24, height / 24)
        c.strokeStyle = tint; c.fillStyle = tint; c.lineWidth = 1.8; c.lineCap = "round"; c.lineJoin = "round"
        function line(points) { c.beginPath(); c.moveTo(points[0], points[1]); for(var i=2;i<points.length;i+=2)c.lineTo(points[i],points[i+1]); c.stroke() }
        function circle(x,y,r) {c.beginPath();c.arc(x,y,r,0,Math.PI*2);c.stroke()}
        switch(name) {
        case "overview": line([2,11,12,3,22,11]); line([5,9,5,21,10,21,10,15,14,15,14,21,19,21,19,9]); break
        case "miner": case "cpu": c.strokeRect(6,6,12,12);c.strokeRect(9,9,6,6);for(var j=7;j<=17;j+=5){line([j,2,j,5]);line([j,19,j,22]);line([2,j,5,j]);line([19,j,22,j])}break
        case "rewards": c.fillRect(4,12,3,9);c.fillRect(10,7,3,14);c.fillRect(16,3,3,18);break
        case "wallet": c.strokeRect(3,6,18,14);line([4,6,4,3,18,3,18,6]);line([21,11,15,11,15,16,21,16]);circle(17.5,13.5,0.4);break
        case "proofs": line([5,2,15,2,20,7,20,22,5,22,5,2]);line([14,2,14,8,20,8]);line([8,12,16,12]);line([8,16,15,16]);break
        case "nodes": c.beginPath();c.ellipse(3,3,18,6);c.stroke();line([3,6,3,19]);line([21,6,21,19]);for(var y=12;y<=19;y+=7){c.beginPath();c.ellipse(3,y-3,18,6);c.stroke()}break
        case "security": line([12,2,21,6,20,14,17,19,12,22,7,19,4,14,3,6,12,2]);break
        case "settings": circle(12,12,4);for(var k=0;k<8;k++){var a=k*Math.PI/4;line([12+8*Math.cos(a),12+8*Math.sin(a),12+10*Math.cos(a),12+10*Math.sin(a)])}circle(12,12,8);break
        case "help": circle(12,12,9);c.beginPath();c.arc(12,9,3,Math.PI,Math.PI*2+Math.PI/2);c.stroke();line([12,12,12,14]);circle(12,18,0.4);break
        case "copy": c.strokeRect(8,8,12,13);line([16,8,16,3,3,3,3,16,8,16]);break
        case "external":line([14,3,21,3,21,10]);line([21,3,10,14]);line([10,4,4,4,4,21,20,21,20,14]);break
        case "lock":c.strokeRect(5,10,14,11);c.beginPath();c.arc(12,10,4,Math.PI,0);c.stroke();line([12,14,12,17]);break
        case "eye": case "eyeOff":c.beginPath();c.moveTo(2,12);c.quadraticCurveTo(12,1,22,12);c.quadraticCurveTo(12,23,2,12);c.stroke();circle(12,12,3);if(name==="eyeOff")line([3,3,21,21]);break
        case "play":line([7,3,21,12,7,21,7,3]);break
        case "pause":c.fillRect(6,4,4,16);c.fillRect(14,4,4,16);break
        case "stop":c.fillRect(5,5,14,14);break
        case "plus":line([12,4,12,20]);line([4,12,20,12]);break
        case "check":line([4,12,10,18,21,5]);break
        case "arrow":line([3,12,21,12]);line([15,6,21,12,15,18]);break
        case "back":line([21,12,3,12]);line([9,6,3,12,9,18]);break
        case "download":line([12,3,12,16]);line([6,10,12,16,18,10]);line([3,17,3,22,21,22,21,17]);break
        case "chevron":line([6,9,12,15,18,9]);break
        case "close":line([5,5,19,19]);line([19,5,5,19]);break
        case "minus":line([4,12,20,12]);break
        case "maximize":c.strokeRect(5,5,14,14);break
        case "clock":circle(12,12,9);line([12,6,12,12,17,14]);break
        case "block":line([12,2,22,7,22,17,12,22,2,17,2,7,12,2,12,22]);line([2,7,12,12,22,7]);break
        default: circle(12,12,9);line([12,6,12,13]);circle(12,17,0.5)
        }
    }
}
