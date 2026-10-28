"""pack.py <file>... <archive>

Writes the Python files among its arguments into one archive python3 can run,
each under its own name, and skips the rest. Every entry carries the same time
and mode, so the archive is the same bytes wherever it is built.
"""

import os
import sys
import zipfile

*files, archive = sys.argv[1:]
sources = sorted((p for p in files if p.endswith(".py")), key=os.path.basename)
with zipfile.ZipFile(archive, "w", zipfile.ZIP_STORED) as z:
    for path in sources:
        entry = zipfile.ZipInfo(os.path.basename(path), date_time=(1980, 1, 1, 0, 0, 0))
        entry.external_attr = 0o644 << 16
        with open(path, "rb") as f:
            z.writestr(entry, f.read())
