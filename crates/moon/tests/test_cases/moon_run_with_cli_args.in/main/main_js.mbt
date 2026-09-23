pub fn get_args() -> Array[String] {
  Array::from_fixed_array(get_cli_args_internal())
}

extern "js" fn get_cli_args_internal() -> FixedArray[String] =
  #| function() {
  #|  return process.argv;
  #| }

fn main {
  let args = get_args()
  for i = 0; i < args.length(); i = i + 1 {
    println(args[i])
  }
}
