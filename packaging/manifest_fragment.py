#!/usr/bin/env python3
"""Hash one release artifact; write its .sha256 and a manifest fragment.

Usage: manifest_fragment.py <artifact> <platform-key> <base-url>

Used by release.yml — python's hashlib runs identically on every runner,
unlike sha256sum(1) which macOS does not ship. The fragment JSON feeds
assemble_manifest.py (schema 1, doc 15 §2.1).
"""

import hashlib
import json
import os
import sys


def main() -> None:
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    artifact, key, base = sys.argv[1:4]
    if not os.path.isfile(artifact):
        sys.exit(f"artifact not found: {artifact}")

    digest = hashlib.sha256()
    with open(artifact, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    sha = digest.hexdigest()

    with open(artifact + ".sha256", "w", encoding="utf-8") as out:
        out.write(f"{sha}  {artifact}\n")

    fragment = {
        "key": key,
        "artifact": os.path.basename(artifact),
        "url": f"{base.rstrip('/')}/{os.path.basename(artifact)}",
        "sha256": sha,
        "size": os.path.getsize(artifact),
    }
    with open(f"frag-{key}.json", "w", encoding="utf-8") as out:
        json.dump(fragment, out, indent=2)
        out.write("\n")

    print(f"{artifact}: sha256={sha} size={fragment['size']}")


if __name__ == "__main__":
    main()
