#!/usr/bin/env python3
"""Merge manifest fragments into the published update manifest (doc 15 §2.1).

Usage: assemble_manifest.py <meta-dir> <version-tag> <output.json>

<version-tag> is the git tag (v1.2.3); the manifest records it without the
leading "v". Each frag-*.json in <meta-dir> is one platform entry.
"""

import glob
import json
import os
import sys


def main() -> None:
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    meta_dir, version_tag, out_path = sys.argv[1:4]

    fragments = sorted(glob.glob(os.path.join(meta_dir, "frag-*.json")))
    if not fragments:
        sys.exit(f"no manifest fragments in {meta_dir}")

    platforms = {}
    for path in fragments:
        with open(path, encoding="utf-8") as f:
            frag = json.load(f)
        platforms[frag["key"]] = {
            field: frag[field] for field in ("artifact", "url", "sha256", "size")
        }

    manifest = {
        "schema": 1,
        "product": "aethel",
        "version": version_tag.lstrip("v"),
        "platforms": platforms,
    }
    with open(out_path, "w", encoding="utf-8") as out:
        json.dump(manifest, out, indent=2)
        out.write("\n")
    print(f"wrote {out_path} with {len(platforms)} platform(s)")


if __name__ == "__main__":
    main()
