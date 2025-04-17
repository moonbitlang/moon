fn main {
  let triangle : Array[Array[Int]] = [[2], [3, 4], [6, 5, 7], [4, 1, 8, 3]]
  let result = @lib.minPathSum(triangle)
  println(result) // prints 11
}
