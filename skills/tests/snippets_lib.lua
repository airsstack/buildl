-- Finds the lua blocks of a Markdown text that `cargo make skills` checks with `buildl check`.
-- A block is checked when its opening fence's info string is exactly `lua`; `lua no-check`
-- and every other language are left out.
local M = {}

-- Returns the checkable blocks of `text` in document order, each `{ body, index, line }`:
-- `index` counts checkable blocks from 1, `line` is where the fence opens. `name` names the
-- text in the error raised for a fence that never closes.
function M.blocks(text, name)
    local found = {}
    local open -- the fence being read: { ticks, info, line, lines }
    local n = 0
    for raw in (text .. "\n"):gmatch("(.-)\r?\n") do
        n = n + 1
        if open == nil then
            local ticks, info = raw:match("^%s*(```+)%s*(.-)%s*$")
            if ticks then
                open = { ticks = #ticks, info = info, line = n, lines = {} }
            end
        else
            local ticks = raw:match("^%s*(```+)%s*$")
            if ticks and #ticks >= open.ticks then
                if open.info == "lua" then
                    found[#found + 1] = {
                        body = table.concat(open.lines, "\n") .. "\n",
                        index = #found + 1,
                        line = open.line,
                    }
                end
                open = nil
            else
                open.lines[#open.lines + 1] = raw
            end
        end
    end
    if open ~= nil then
        error(string.format("%s: line %d: code fence is never closed", name, open.line), 0)
    end
    return found
end

return M
