const {
  moonbit_test_driver_internal_execute,
  moonbit_test_driver_finish,
} = require('origin_js_path')

let packageName = ''
let testParams = []

try {
  // check if there is command line argument
  if (process.argv.length > 2) {
    const testArgs = JSON.parse(process.argv[2])
    packageName = testArgs.package
    testParams = testArgs.file_and_index.flatMap(([file, ranges]) => {
      let arr = []
      for (let range of ranges) {
        for (let i = range.start; i < range.end; i++) {
          arr.push([file, i])
        }
      }
      return arr
    })
  }
} catch (error) {
  console.error('failed to parse args: ', error.message)
  process.exit(1)
}

for (param of testParams) {
  try {
    moonbit_test_driver_internal_execute(param[0], param[1])
  } catch (e) {
    console.log('----- BEGIN MOON TEST RESULT -----')
    console.log(
      `{"package": "${packageName}", "filename": "${param[0]}", "index": "${
        param[1]
      }", "test_name": "${param[1]}", "message": "${e.stack
        .toString()
        .replaceAll('\\', '\\\\')
        .split('\n')
        .join('\\n')}"}`
    )
    console.log('----- END MOON TEST RESULT -----')
  }
}
moonbit_test_driver_finish()
