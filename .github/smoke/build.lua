local b = buildl

b.target("upper", {
  inputs = { "greeting.txt" },
  run = { "sh", "-c", "tr a-z A-Z < greeting.txt > \"$1\"", "sh", "$out" },
})

b.test("reads", {
  inputs = { "greeting.txt" },
  run = { "sh", "-c", "grep -q hello greeting.txt" },
})
