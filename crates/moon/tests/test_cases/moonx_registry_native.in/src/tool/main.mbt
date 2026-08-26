///|
fn main {
  if @env.get_env_var("MOONRUN_INHERITED_POLICY") is Some(_) {
    abort("policy relay leaked into native execution")
  }
  println("native runner")
  for i = 1; i < @env.args().length(); i = i + 1 {
    println(@env.args()[i])
  }
}
