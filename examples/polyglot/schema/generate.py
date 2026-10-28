"""generate.py <schema.json> <go> <go.mod> <rust> <python>

Writes the schema's values as Go, Rust and Python source. Only the named values
reach the output, so an edit to the schema's comment writes the same bytes.
"""

import json
import sys

schema_path, go_path, go_mod_path, rust_path, python_path = sys.argv[1:]
with open(schema_path, encoding="utf-8") as f:
    schema = json.load(f)
version = json.dumps(schema["version"])
greeting = json.dumps(schema["greeting"])

outputs = {
    go_path: (
        "// Generated from schema.json. Do not edit.\n"
        "package schema\n\n"
        f"const Version = {version}\n"
        f"const Greeting = {greeting}\n"
    ),
    go_mod_path: "module example.com/polyglot/schema\n\ngo 1.22\n",
    rust_path: (
        "// Generated from schema.json. Do not edit.\n"
        f"pub const VERSION: &str = {version};\n"
        f"pub const GREETING: &str = {greeting};\n"
    ),
    python_path: (
        "# Generated from schema.json. Do not edit.\n"
        f"VERSION = {version}\n"
        f"GREETING = {greeting}\n"
    ),
}
for path, text in outputs.items():
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
