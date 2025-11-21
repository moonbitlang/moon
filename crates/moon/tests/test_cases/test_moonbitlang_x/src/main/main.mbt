///|
fn main {
  let stk = @stack.new()
  stk.push(123)
  stk.peek() |> println
}
