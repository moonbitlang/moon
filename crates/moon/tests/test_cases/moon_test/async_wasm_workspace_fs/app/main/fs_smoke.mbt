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

///|
async test "directory entries and timestamps through async fs" {
  let dir = "async-fs-smoke-dir"
  let path = "\{dir}/entry.txt"
  @fs.mkdir(dir)
  @fs.write_file(path, b"entry", create_mode=CreateOrTruncate)
  let entries = @fs.readdir(dir, sort=true)
  json_inspect(entries, content=["entry.txt"])
  let (path_sec, path_nsec) = @fs.mtime(path)
  assert_true(path_sec >= 0)
  assert_true(path_nsec >= 0)
  let file = @fs.open(path, mode=ReadOnly)
  defer file.close()
  let (file_sec, file_nsec) = file.mtime()
  assert_true(file_sec >= 0)
  assert_true(file_nsec >= 0)
}
