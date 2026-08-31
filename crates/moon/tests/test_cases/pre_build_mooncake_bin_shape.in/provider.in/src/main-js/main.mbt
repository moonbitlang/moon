extern "js" fn record_mooncake_bin_path() -> Unit =
  #|function() {
  #|  // The prebuild rule passes its output and expanded $mooncake_bin path.
  #|  require("fs").writeFileSync(process.argv[2], process.argv[3]);
  #|}

fn main {
  record_mooncake_bin_path()
}
