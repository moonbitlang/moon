
test {
  println("bb test")
  @lib1.f1(1)
  @lib3.f3()
  abort("-------------- abort msg --------------")
}