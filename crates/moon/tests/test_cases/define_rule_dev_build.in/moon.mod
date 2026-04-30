name = "username/define_rule_dev_build"

rule(
  name: "main_source",
  command: "cat $input >/dev/null && echo 'fn main { println(helper()) }' > $output",
)

rule(
  name: "helper_source",
  command: "cat $input >/dev/null && echo 'fn helper() -> Int { 42 }' > $output",
)
