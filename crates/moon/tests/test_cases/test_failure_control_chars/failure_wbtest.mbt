///|
test "ANSI color" {
  fail("a\u{1b}[31mred\u{1b}[0m")
}

///|
test "other escapes" {
  let controls = "\u{0}\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\t\n\u{b}\u{c}\r\u{e}\u{f}" +
    "\u{10}\u{11}\u{12}\u{13}\u{14}\u{15}\u{16}\u{17}\u{18}\u{19}\u{1a}\u{1b}\u{1c}\u{1d}\u{1e}\u{1f}"
  fail("\{controls}\"\\你好🌙")
}

///|
test "still runs" {
  assert_eq(1, 1)
}
