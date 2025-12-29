///|
pub fn f(n : String) -> String {
  (try? @strconv.parse_int(n)).to_string()
}
