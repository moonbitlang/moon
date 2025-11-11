///|
fn main {
  @lib.hello() |> ignore
}

///|
fn[T] ignore(t : T) = "%ignore"
