test "hello" {
  let buf = Buffer::make(10)
  buf.write_string("just\ntest")
  buf.expect()?
}

test "not-buf" {
  let notbuf = Buffer::make(10)
  notbuf.write_string("haha")
  notbuf.expect()?
}
