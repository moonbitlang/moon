const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')

// Environment-only scripts do not need to read the compatibility stdin payload.
assert.equal(process.env.MOON_MOD, path.join(process.cwd(), 'moon.mod.json'))
assert.ok(path.isAbsolute(process.env.MOON_BUILD_DIR))
assert.ok(fs.statSync(process.env.MOON_BUILD_DIR).isDirectory())
fs.writeFileSync(path.join(process.env.MOON_BUILD_DIR, 'generated.txt'), 'ready')
assert.ok(process.env.MOON_CC)
assert.equal(process.env.MOON_BACKEND, 'native')
assert.equal(process.env.MOON_PROFILE, 'debug')
assert.equal(process.env.MOON_JOBS, '3')

const output = {
  vars: {
    HELLO: '------this-is-added-by-config-script------',
  },
  link_configs: [
    {
      package: 'username/hello/dep',
      link_flags: '-l______this_is_added_by_config_script_______',
      link_libs: ['mylib'],
      link_search_paths: ['/my-search-path'],
    },
  ],
}
console.log(JSON.stringify(output))
