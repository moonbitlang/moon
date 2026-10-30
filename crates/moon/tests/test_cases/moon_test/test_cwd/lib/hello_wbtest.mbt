extern "js" fn read_text_file(path : String) -> String =
  #| function(path) {
  #|   return require("fs").readFileSync(path, "utf8");
  #| }

test "runs from module root" {
  assert_eq(read_text_file("cwd-marker.txt"), "module-root-cwd\n")
}
