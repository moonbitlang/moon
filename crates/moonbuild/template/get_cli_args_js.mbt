// return type can't be Array[String], which will lead to empty result
extern "js" fn _get_cli_args() -> FixedArray[String] =
  #| function(key) {
  #|  return process.argv;
  #| }

fn get_cli_args() -> Array[String] {
  Array::from_fixed_array(_get_cli_args())
}