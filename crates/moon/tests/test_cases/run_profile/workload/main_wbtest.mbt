///|
test "profile workload" {
  println("cwd: \{@env.current_dir().unwrap()}")
  workload()
}
