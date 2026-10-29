-- Tests of snippets_lib. Run from this folder: airsl test --allow-read . .
local snippets = require("snippets_lib")

local SAMPLE = "fixtures/snippets/sample.md"

local function sample_blocks()
    return snippets.blocks(airsstack.fs.read(SAMPLE), SAMPLE)
end

local function any_body_holds(blocks, text)
    for _, b in ipairs(blocks) do
        if b.body:find(text, 1, true) then
            return true
        end
    end
    return false
end

return {
    a_lua_block_is_returned = function()
        local b = sample_blocks()[1]
        assert(b.body:find('buildl.target("first"', 1, true), "first body: " .. b.body)
        assert(b.index == 1, "index " .. tostring(b.index))
        assert(b.line == 5, "line " .. tostring(b.line))
    end,

    a_no_check_block_is_skipped = function()
        assert(not any_body_holds(sample_blocks(), '"skipped"'))
    end,

    other_languages_are_skipped = function()
        local blocks = sample_blocks()
        assert(not any_body_holds(blocks, "toml-body"))
        assert(not any_body_holds(blocks, "text-body"))
    end,

    blocks_keep_document_order = function()
        local blocks = sample_blocks()
        assert(#blocks == 2, "count " .. #blocks)
        assert(blocks[2].index == 2)
        assert(blocks[2].body:find('buildl.target("second"', 1, true))
    end,

    an_unclosed_fence_is_an_error = function()
        local ok, err = pcall(snippets.blocks, "intro\n\n```lua\nbuildl.target(\"x\", {})\n", "open.md")
        assert(not ok, "expected an error")
        err = tostring(err)
        assert(err:find("open.md", 1, true), err)
        assert(err:find("line 3:", 1, true), err)
    end,
}
