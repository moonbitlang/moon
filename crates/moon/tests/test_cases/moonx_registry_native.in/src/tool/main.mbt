///|
fn main {
  println("native runner")
  for i = 1; i < @env.args().length(); i = i + 1 {
    println(@env.args()[i])
  }
}
