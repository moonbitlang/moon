///|
async test "write and read file through async fs" {
  let path = "async-fs-smoke.txt"
  @fs.write_file(path, b"wasm async fs", create_mode=CreateOrTruncate)
  let content = @fs.read_file(path).text()
  inspect(content, content="wasm async fs")
  {
    let file = @fs.open(path, mode=ReadWrite)
    defer file.close()
    file.write_at(b"IO", position=5)
    let buf = FixedArray::make(2, b'\x00')
    let n = file.read_at(buf, position=5)
    inspect(n, content="2")
    json_inspect(buf.unsafe_reinterpret_as_bytes()[:n], content="IO")
  }
  let content = @fs.read_file(path).text()
  inspect(content, content="wasm IOync fs")
}
