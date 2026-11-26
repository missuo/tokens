#!/usr/bin/env python3
"""Point the Homebrew formula at a new release.

    bump-homebrew-formula.py <Formula/tokens.rb> <version> <dist-dir>

`dist-dir` holds the release tarballs, named
`tokens-v<version>-<target>.tar.gz` as `publish-cli.yml` builds them. Every
`url` in the formula names one target; the `sha256` on the line after it is
replaced with that tarball's digest, and `version` is set. The URLs themselves
interpolate `#{version}`, so they are left alone.

Fails, without writing, if the formula has no `version` line, if a `url` is
not followed by a `sha256`, or if a target it names has no tarball. A formula
that no longer matches this layout should stop the release step rather than be
half updated.
"""

import hashlib
import pathlib
import re
import sys

URL = re.compile(r'url "[^"]*-v#\{version\}-([A-Za-z0-9_.-]+)\.tar\.gz"')
SHA = re.compile(r'(sha256 ")[0-9a-f]{64}(")')
VERSION = re.compile(r'^(\s*version ")[^"]+(")', re.MULTILINE)


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bump(formula: str, version: str, dist: pathlib.Path) -> str:
    formula, count = VERSION.subn(rf"\g<1>{version}\g<2>", formula, count=1)
    if count != 1:
        sys.exit("formula has no version line")

    lines = formula.split("\n")
    targets = []
    for i, line in enumerate(lines):
        match = URL.search(line)
        if not match:
            continue
        target = match.group(1)
        if i + 1 >= len(lines) or not SHA.search(lines[i + 1]):
            sys.exit(f"url for {target} is not followed by a sha256 line")
        tarball = dist / f"tokens-v{version}-{target}.tar.gz"
        if not tarball.is_file():
            sys.exit(f"missing {tarball}")
        lines[i + 1] = SHA.sub(rf"\g<1>{sha256(tarball)}\g<2>", lines[i + 1])
        targets.append(target)

    if not targets:
        sys.exit("formula has no release urls")
    print("updated: " + ", ".join(targets), file=sys.stderr)
    return "\n".join(lines)


def main() -> None:
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    path = pathlib.Path(sys.argv[1])
    path.write_text(bump(path.read_text(), sys.argv[2], pathlib.Path(sys.argv[3])))


if __name__ == "__main__":
    main()
