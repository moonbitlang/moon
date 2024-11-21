pub fn get_args() -> Array[String] {
  get_cli_args_internal()
}

extern "js" fn get_cli_args_internal() -> Array[String] =
  #| function() {
  #|  return process.argv.slice(2);
  #| }

fn main {
  get_args() |> println
}