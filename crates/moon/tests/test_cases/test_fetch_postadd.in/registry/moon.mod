name = "testuser/postadd"
version = "1.0.0"
source = "src"
options(scripts: {
  "postadd": "moon tool embed --text -i src/tool/generated.txt -o src/tool/generated.mbt --name generated",
})
