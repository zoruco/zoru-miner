.pragma library
var colors = {
    background: "#0b0f12", panel: "#11161a", elevated: "#191f25", border: "#293139",
    text: "#f3f5f7", muted: "#a4aeba", faint: "#727f8d", orange: "#ff7b26",
    orangeLight: "#ffa04b", green: "#56ed91", red: "#ff6673", yellow: "#ffb657"
}
function color(name) { return colors[name] || colors.text }
var typography = {page:48,compactPage:35,subtitle:17,section:19,body:14,caption:13,small:12,value:23,emptyTitle:15}
function size(role) { return typography[role] || typography.body }

// Keep financial strings exact; converting wei-derived balances to Number loses precision.
function amountSummary(value, decimals) {
    var text = String(value === undefined || value === null ? "—" : value)
    var match = /^(-?)(\d+)(?:\.(\d+))?$/.exec(text)
    if (!match) return text
    var whole = match[2].replace(/^0+(?=\d)/, "")
    var fraction = (match[3] || "").replace(/0+$/, "")
    var places = decimals === undefined ? 4 : Math.max(0, decimals)
    var omitted = fraction.slice(places)
    var approximate = /[1-9]/.test(omitted)
    fraction = fraction.slice(0, places).replace(/0+$/, "")
    if (whole === "0" && !/[1-9]/.test(fraction) && approximate)
        return (match[1] ? "−" : "") + (places ? "<0." + new Array(places).join("0") + "1" : "<1")
    return (approximate ? "≈ " : "") + match[1] + whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",") + (fraction ? "." + fraction : "")
}
function contains(values, value) { return values.indexOf(value) >= 0 }
function rewardPaid(row) {
    return ["Reward paid"].indexOf(row.status) >= 0
        && Number(row.reward || 0) > 0
}
function failedProof(row) {
    return ["Rejected", "Preflight rejected", "Fee limit exceeded", "Reverted", "Stale", "Reorged", "Cancelled", "Failed", "Expired"].indexOf(row.status) >= 0
}
