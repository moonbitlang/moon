///|
extern "C" fn twice_ffi(x : Int) -> Int = "dep_twice"

///|
pub fn twice(x : Int) -> Int {
  twice_ffi(x)
}
