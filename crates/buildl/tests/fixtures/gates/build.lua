-- Eight targets with nothing between them, so a build can run them all at once.
for i = 0, 7 do
  buildl.target("t" .. i, { inputs = { "in.txt" }, run = { "cp", "$in", "$out" } })
end
