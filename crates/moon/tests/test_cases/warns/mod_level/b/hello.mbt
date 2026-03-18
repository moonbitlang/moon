test {
  internal_one();
  internal_two();
}

#internal(one, "one")
pub fn internal_one() -> Unit {
}

#internal(two, "two")
pub fn internal_two() -> Unit {
}
