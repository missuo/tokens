#!/bin/sh
# Keep every recent deploy's static assets servable, not just this one's.
#
# Next names each chunk after a hash of its contents, so every build reissues
# them under new names, and the old files vanish with the container they shipped
# in. Meanwhile the HTML referencing them outlives the deploy: in a CDN edge
# entry, and in every browser tab already open. When such a document asks for a
# chunk the running image no longer has, the origin 404s it as text/plain, the
# browser refuses to execute it, and Next replaces the page with its
# client-side exception screen. It surfaces per edge location, so it reads as
# though it depended on the visitor's IP.
#
# The fix is to stop deleting the old assets. `.next/static` is a volume (see
# compose.yaml) and this script layers each deploy's files into it, so a
# document from a previous build still resolves every chunk it names.
set -eu

TARGET=/app/web/.next/static
SOURCE=/app/static-dist
# Longer than any edge entry lives or any tab stays open, and the copy below
# refreshes the mtime of everything still shipping — so the sweep only removes
# assets that no recent deploy contained.
RETENTION_DAYS=30

if [ -d "$SOURCE" ]; then
  # Overwrite rather than skip. It costs ~2 MB of writes and buys two things:
  # this deploy's assets are in place even if an earlier run left the volume
  # half-written, and their mtimes become "now", which is what makes the sweep
  # below mean "last shipped" instead of "first written". A name that already
  # exists is rewritten with identical bytes, because the name *is* the hash.
  # Files from earlier deploys are absent from SOURCE and so are left alone —
  # that is the accumulation this exists for.
  #
  # Never fatal: serving the site matters more than pruning it, so a failure
  # here is reported and the server still starts.
  cp -R "$SOURCE/." "$TARGET/" 2>/dev/null \
    || echo "entrypoint: could not sync static assets into $TARGET" >&2
fi

find "$TARGET" -type f -mtime "+$RETENTION_DAYS" -delete 2>/dev/null || true
find "$TARGET" -mindepth 1 -type d -empty -delete 2>/dev/null || true

exec "$@"
