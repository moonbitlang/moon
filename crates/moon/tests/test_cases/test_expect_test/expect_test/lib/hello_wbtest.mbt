test "hello" {
  let buf = @buffer.new()
  buf.write_string_utf16le("just\ntest")
  inspect(buf)
}

test "not-buf" {
  let notbuf = @buffer.new()
  notbuf.write_string_utf16le("haha")
  inspect(notbuf)
}
