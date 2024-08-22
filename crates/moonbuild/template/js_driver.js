const { execute } = require("origin_js_path");

const package = "";
const test_params = [];

for (param of test_params) {
    try {
        execute(param[0], parseInt(param[1]));
    } catch (e) {
        console.log("----- BEGIN MOON TEST RESULT -----")
        console.log(`{"package": "${package}", "filename": "${param[0]}", "index": "${param[1]}", "test_name": "${param[1]}", "message": "${e.stack.toString().split('\n').join('\\n')}"}`);
        console.log("----- END MOON TEST RESULT -----")
    }
}
