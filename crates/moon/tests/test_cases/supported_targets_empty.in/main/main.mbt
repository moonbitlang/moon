#cfg(target="js")
fn main {
  ignore(@lib.banner())
}

#cfg(target="native")
fn main {
  ignore(@lib.banner())
}
