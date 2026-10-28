local b = buildl

-- Each directory declares its own targets; they reach each other by label.
b.subdir("schema")
b.subdir("go")
b.subdir("rust")
b.subdir("python")
