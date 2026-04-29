name = "username/define_rule_dev_build"

define_rule(name: "main_source", command: "cat $input >/dev/null && printf '///|\nfn main { println(helper()) }\n' > $output")
define_rule(name: "helper_source", command: "printf '///|\nfn helper() -> String { \"%s\" }\n' \"$(cat $input)\" > $output")
