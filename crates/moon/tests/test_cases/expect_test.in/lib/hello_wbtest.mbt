test "hello" {
  let buf = @buffer.new()
  buf.write_string("just\ntest")
  inspect!(buf)
}

test "not-buf" {
  let notbuf = @buffer.new()
  notbuf.write_string("haha")
  inspect!(notbuf)
}
