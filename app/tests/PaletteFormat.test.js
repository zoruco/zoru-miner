const assert = require("node:assert/strict")
const fs = require("node:fs")
const path = require("node:path")
const vm = require("node:vm")

const source = fs.readFileSync(path.join(__dirname, "../qml/Palette.js"), "utf8")
    .replace(/^\.pragma library\s*/, "")
const context = vm.createContext({})
vm.runInContext(source, context)

for (const [input, precision, expected] of [
    ["21170", 8, "21,170"],
    ["21145", 8, "21,145"],
    ["0", 8, "0"],
    ["123456789012345678901234567890", 8, "123,456,789,012,345,678,901,234,567,890"],
    ["-21170.50000000", 8, "-21,170.5"],
    ["00021170.00000000", 8, "21,170"],
    ["0.000000001", 8, "<0.00000001"],
    ["1234.567899999", 8, "≈ 1,234.56789999"],
]) {
    assert.equal(context.amountSummary(input, precision), expected, input)
}
console.log("PASS: exact financial formatting uses one grouping separator")
