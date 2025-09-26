test {
  println("I am always executed")
}

#cfg(false)
test {
  println("I am never executed")
}

#cfg(target="wasm")
test {
  println("I only execute on wasm")
}

#cfg(not(target="wasm"))
test {
  println("I only execute on not wasm")
}

