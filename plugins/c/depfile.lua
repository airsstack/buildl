-- Reads the Make rules a compiler's -MM writes, and normalises the paths in them.
-- Pure: it uses no `buildl`.
local M = {}

-- Splits the text after a rule's colon on whitespace. `\ ` (a backslash and a space) is a
-- space in the path; any other whitespace splits.
local function words(text)
    local found = {}
    local word = {}
    local i = 1
    local n = #text
    while i <= n do
        local c = text:sub(i, i)
        if c == "\\" and text:sub(i + 1, i + 1) == " " then
            word[#word + 1] = " "
            i = i + 2
        else
            if c:match("%s") then
                if #word > 0 then
                    found[#found + 1] = table.concat(word)
                    word = {}
                end
            else
                word[#word + 1] = c
            end
            i = i + 1
        end
    end
    if #word > 0 then
        found[#found + 1] = table.concat(word)
    end
    return found
end

-- Returns the rules of `text` in input order, each `{ target, prerequisites }`. A `\` and the
-- newline after it join two lines; a line with no colon is skipped.
function M.parse(text)
    local joined = text:gsub("\\\r?\n", " ")
    local rules = {}
    for line in (joined .. "\n"):gmatch("(.-)\r?\n") do
        local target, rest = line:match("^%s*(.-):%s(.*)$")
        if target == nil then
            target, rest = line:match("^%s*(.-):$"), ""
        end
        if target ~= nil and target ~= "" then
            rules[#rules + 1] = { target = target, prerequisites = words(rest) }
        end
    end
    return rules
end

-- Returns `path` without its `.` segments and empty ones, each `..` having removed the
-- segment before it. A leading `/` is kept; a relative path that reduces to nothing comes back
-- as ".". Raises, naming `c.depfile.normalise` and `path`, when a `..` has no segment to remove.
function M.normalise(path)
    local kept = {}
    for segment in path:gmatch("[^/]+") do
        if segment == ".." then
            if #kept == 0 then
                error(string.format("c.depfile.normalise: the path %s climbs above the start", path), 2)
            end
            kept[#kept] = nil
        elseif segment ~= "." then
            kept[#kept + 1] = segment
        end
    end
    local joined = table.concat(kept, "/")
    if path:sub(1, 1) == "/" then
        return "/" .. joined
    end
    if joined == "" then
        return "."
    end
    return joined
end

return M
