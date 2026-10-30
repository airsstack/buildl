-- Tests of depfile. Run from a folder holding the module: airsl test --allow-read . .
local depfile = require("depfile")

return {
    one_rule_is_read = function()
        local rules = depfile.parse("obj: src/a.c include/a.h\n")
        assert(#rules == 1, "rules " .. #rules)
        assert(rules[1].target == "obj", "target " .. tostring(rules[1].target))
        local p = rules[1].prerequisites
        assert(#p == 2, "prerequisites " .. #p)
        assert(p[1] == "src/a.c", p[1])
        assert(p[2] == "include/a.h", p[2])
    end,

    a_continuation_is_joined = function()
        local rules = depfile.parse("obj: src/a.c \\\n  include/a.h \\\n  include/b.h\n")
        assert(#rules == 1, "rules " .. #rules)
        local p = rules[1].prerequisites
        assert(#p == 3, "prerequisites " .. #p)
        assert(p[1] == "src/a.c", p[1])
        assert(p[2] == "include/a.h", p[2])
        assert(p[3] == "include/b.h", p[3])
    end,

    an_escaped_space_is_kept_in_the_path = function()
        local rules = depfile.parse("obj: a.c my\\ dir/sp.h\n")
        local p = rules[1].prerequisites
        assert(#p == 2, "prerequisites " .. #p)
        assert(p[2] == "my dir/sp.h", p[2])
    end,

    several_rules_are_kept_in_order = function()
        local rules = depfile.parse("first: a.c a.h\nsecond: b.c b.h\n")
        assert(#rules == 2, "rules " .. #rules)
        assert(rules[1].target == "first", rules[1].target)
        assert(rules[1].prerequisites[2] == "a.h")
        assert(rules[2].target == "second", rules[2].target)
        assert(rules[2].prerequisites[2] == "b.h")
    end,

    normalise_drops_dot_and_resolves_dotdot = function()
        assert(depfile.normalise("cpp/src/../include/u.h") == "cpp/include/u.h")
        assert(depfile.normalise("./a/./b.h") == "a/b.h")
    end,

    normalise_refuses_a_climb_above_the_root = function()
        local ok, err = pcall(function()
            return (depfile.normalise("../x.h"))
        end)
        assert(not ok, "expected an error")
        err = tostring(err)
        assert(err:find("c.depfile.normalise", 1, true), err)
        assert(err:find("../x.h", 1, true), err)
        assert(err:find(":%d+:"), "no location in " .. err)
    end,

    normalise_keeps_a_leading_slash = function()
        assert(depfile.normalise("/usr/include/../lib/x.h") == "/usr/lib/x.h")
        assert(depfile.normalise("/a/./b.h") == "/a/b.h")
    end,

    normalise_returns_a_dot_for_a_path_that_reduces_to_nothing = function()
        assert(depfile.normalise(".") == ".")
        assert(depfile.normalise("a/..") == ".")
        assert(depfile.normalise("./") == ".")
    end,

    crlf_line_endings_are_read = function()
        local rules = depfile.parse("first: a.c \\\r\n  a.h\r\nsecond: b.c\r\n")
        assert(#rules == 2, "rules " .. #rules)
        assert(rules[1].target == "first", rules[1].target)
        local p = rules[1].prerequisites
        assert(#p == 2, "prerequisites " .. #p)
        assert(p[1] == "a.c", p[1])
        assert(p[2] == "a.h", p[2])
        assert(rules[2].target == "second", rules[2].target)
        assert(#rules[2].prerequisites == 1, "prerequisites " .. #rules[2].prerequisites)
        assert(rules[2].prerequisites[1] == "b.c", rules[2].prerequisites[1])
    end,

    a_rule_with_no_prerequisites_is_kept = function()
        local rules = depfile.parse("obj:\n")
        assert(#rules == 1, "rules " .. #rules)
        assert(rules[1].target == "obj", rules[1].target)
        assert(#rules[1].prerequisites == 0, "prerequisites " .. #rules[1].prerequisites)
    end,

    a_line_with_no_colon_is_skipped = function()
        local rules = depfile.parse("just words here\nobj: a.c\n")
        assert(#rules == 1, "rules " .. #rules)
        assert(rules[1].target == "obj", rules[1].target)
    end,
}
