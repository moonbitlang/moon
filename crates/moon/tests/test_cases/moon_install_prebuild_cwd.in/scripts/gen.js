const fs = require("node:fs")

const output = process.argv[2]
fs.writeFileSync(output, 'fn generated_message() -> String { "from source-root prebuild" }\n')
