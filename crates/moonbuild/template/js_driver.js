const { execute } = require("origin_js_path");

const args = process.argv.slice(2);
const test_params = [];
for (let i = 0; i < args.length; i += 3) {
    if (i + 2 < args.length) {
        test_params.push([args[i], args[i + 1], args[i + 2]]);
    }
}

for (param of test_params) {
    try {
        execute(param[1], parseInt(param[2]));
    } catch (e) {
        console.log("----- BEGIN MOON TEST RESULT -----")
        console.log(`{"package": "${param[0]}", "filename": "${param[1]}", "index": "${param[2]}", "test_name": "--unknown--", "message": "${e.stack.toString().split('\n').join('\\n')}"}`);
        console.log("----- END MOON TEST RESULT -----")
    }
}
