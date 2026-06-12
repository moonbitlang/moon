const fs = require("node:fs")

const output = process.argv[2]
fs.writeFileSync(output, 'fn generated_message() -> String { "from checkout-root prebuild" }\n')
