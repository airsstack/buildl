local b = buildl
b.target("runnable", {                    -- executable output
  inputs = { "echo.sh" },
  run    = { "cp", "$in", "$out" },
})
b.target("text", {                        -- the same bytes, plain; free to run beside runnable
  inputs = { "echo.txt" },
  run    = { "cp", "$in", "$out" },
})
b.target("text-after", {                  -- the same bytes, plain; stored only after runnable is placed
  deps   = { "runnable" },
  inputs = { "echo.txt" },
  run    = { "cp", "$in", "$out" },
})
