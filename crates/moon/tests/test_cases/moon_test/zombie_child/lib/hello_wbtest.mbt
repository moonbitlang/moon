extern "js" fn fs_write_process_pid_sync(path: String) -> Unit = #|(path) => require('fs').writeFileSync(path, String(process.pid))

extern "js" fn timeout(cb: () -> Unit, delay: Int) -> Unit = #|(cb, delay) => setTimeout(cb, delay)

extern "js" fn set_interval(cb: () -> Unit, delay: Int) -> Int = #|(cb, delay) => setInterval(cb, delay)

extern "js" fn clear_interval(id : Int) = #|(intervalID) => clearInterval(intervalID)

test "keeps_process_alive" {
  let pid_file_path = "test_child_pid.txt"
  fs_write_process_pid_sync(pid_file_path)
  let id = set_interval(fn() {
    ()
  }, 100)
  // Sleep for a long time to allow parent to kill us
  timeout(fn() {
    clear_interval(id)
  }, 1000000)
}
