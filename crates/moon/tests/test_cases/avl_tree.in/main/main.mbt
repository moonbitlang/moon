fn main {
  let mut v : @lib.T[Int] = Empty // Create an empty AVL tree with Int type
  let iter = 30

  // Add values from 0 to iter-1 to the AVL tree
  let mut i = 0
  while i < iter {
    v = v.add(i)
    i = i + 1
  }
  let height = v.height()
  println("height of the tree: \{height}")
  v.print_tree()

  // Check values from 0 to iter-1 in the AVL tree
  let mut j = 0
  while j < iter {
    if not(v.mem(j)) {
      println("impossible")
    }
    j = j + 1
  }

  // Remove values from 0 to iter-1 from the AVL tree
  let mut k = 0
  while k < iter {
    v = v.remove(k)
    k = k + 1
  }

  // Tree is empty, removal successful
  match v {
    Empty => println("success")
    Node(_) => println("impossible")
  }
}
