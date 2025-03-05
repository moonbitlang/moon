///|
test {
  inspect!("\x0b")
  inspect!("a\x0b")
  inspect!("a\x00b")
  inspect!("a\x00b\x19")
  inspect!("\na\n\x00\nb\n\x19")
  inspect!("\n\"a\n\x00\nb\"\n\x19")
}

///|
test {
  inspect!("\"abc\"")
  inspect!("\"a\nb\nc\"")
  inspect!("\x0b\"a\nb\nc\"")
}
