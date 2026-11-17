name = "testuser/second"
version = "1.0.0"

options(scripts: {
  "postadd": "moon tool embed --text -i ../hook-input.txt -o generated.mbt --name generated",
})
