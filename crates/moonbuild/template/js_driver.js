const { execute } = require("origin_js_path");

const packageName = "";
const testParams = [];

for (param of testParams) {
    try {
        execute(param[0], parseInt(param[1]));
    } catch (e) {
        console.log("----- BEGIN MOON TEST RESULT -----")
        console.log(`{"package": "${packageName}", "filename": "${param[0]}", "index": "${param[1]}", "test_name": "${param[1]}", "message": "${e.stack.toString().replaceAll("\\", "\\\\").split('\n').join('\\n')}"}`);
        console.log("----- END MOON TEST RESULT -----")
    }
}
