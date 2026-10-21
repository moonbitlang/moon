///|
extern "C" fn inc_ffi(x : Int) -> Int = "lib_inc"

///|
let offset : Int = 2

///|
#export_name("add")
pub fn add(a : Int, b : Int) -> Int {
  @dep.twice(inc_ffi(a)) + b + offset
}
