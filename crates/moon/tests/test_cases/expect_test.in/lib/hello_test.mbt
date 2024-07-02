test "hello" {
  let buf = Buffer::new()
  buf.write_string("just\ntest")
  buf.expect()!
}

test "not-buf" {
  let notbuf = Buffer::new()
  notbuf.write_string("haha")
  notbuf.expect()!
}
