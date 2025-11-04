fn is_windows() -> Int = "__moonbit_sys_unstable" "is_windows"


fn main {
  let is_win = is_windows() 
  println(is_win)
}