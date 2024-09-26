const { moonbit_test_driver_internal_execute } = require("origin_js_path");

let testArgs;
try {
  testArgs = JSON.parse(process.argv[2]);
} catch (error) {
  console.error("failed to parse args:", error.message);
  process.exit(1);
}

const packageName = testArgs.package;
const testParams = testArgs.file_and_index.flatMap(([file, range]) => 
  Array.from({length: range.end - range.start}, (_, i) => [file, (range.start + i).toString()])
);

for (param of testParams) {
    try {
        moonbit_test_driver_internal_execute(param[0], parseInt(param[1]));
    } catch (e) {
        console.log("----- BEGIN MOON TEST RESULT -----")
        console.log(`{"package": "${packageName}", "filename": "${param[0]}", "index": "${param[1]}", "test_name": "${param[1]}", "message": "${e.stack.toString().replaceAll("\\", "\\\\").split('\n').join('\\n')}"}`);
        console.log("----- END MOON TEST RESULT -----")
    }
}
