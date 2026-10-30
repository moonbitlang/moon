test "exit-7" {
  test_exit_with_code(7)
}

fn test_exit_with_code(code : Int) = "wasi_snapshot_preview1" "proc_exit"
