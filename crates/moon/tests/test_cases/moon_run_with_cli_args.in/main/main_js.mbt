pub fn get_args() -> Array[String] {
  get_cli_args_internal()
}

extern "js" fn get_cli_args_internal() -> Array[String] =
  #| function() {
  #|  return process.argv;
  #| }

fn main {
  let args = get_args()
  for i = 0; i < args.length(); i = i + 1 {
    println(args[i])
  }
}
