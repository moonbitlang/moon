const fs = require('node:fs')
const path = require('node:path')

const out = path.join('src', 'main', 'generated_stub.c')
fs.writeFileSync(out, "int prebuild_generated_stub(void) { return 0; }\n")

console.log('{}')
