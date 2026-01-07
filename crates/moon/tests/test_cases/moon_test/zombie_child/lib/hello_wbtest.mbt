extern "js" fn fs_write_file_sync(path: String, content: String) -> Unit = #|(path, content) => require('fs').writeFileSync(path, content)

extern "js" fn timeout(cb: () -> Unit, delay: Int) -> Unit = #|(cb, delay) => setTimeout(cb, delay)

extern "js" fn set_interval(cb: () -> Unit, delay: Int) -> Int = #|(cb, delay) => setInterval(cb, delay)

extern "js" fn clear_interval(id : Int) = #|(intervalID) => clearInterval(intervalID)

test "holds_file_lock" {
  let mut i = 0
  let lock_file_path = "test_lock_file.txt"
  fs_write_file_sync(lock_file_path, "\{i}")
  i += 1
  // Keep renewing lock every 100ms
  let id = set_interval(fn() {
    fs_write_file_sync(lock_file_path, "\{i}")
    i += 1
  }, 100)
  // Sleep for a long time to allow parent to kill us
  timeout(fn() {
    clear_interval(id)
  }, 1000000)
}

