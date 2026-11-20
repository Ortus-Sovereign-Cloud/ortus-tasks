#!/usr/bin/env bash
# Checks out every part named in pins.toml at its pinned commit under
# .deps/<name>, and builds its web client when it has one. A checkout that
# is already at its pin is left alone.
#
# ORTUS_DEPS_CACHE=<dir> keeps one checkout per part and commit in <dir>
# and links .deps/<name> to it, so repositories on one machine share them.
set -euo pipefail
cd "$(dirname "$0")/.."
org="${ORTUS_GIT_BASE:-https://github.com/Ortus-Sovereign-Cloud}"
mkdir -p .deps
grep -E '^[a-z0-9-]+ = "[0-9a-f]{40}"$' pins.toml | while read -r name _ quoted; do
  rev="${quoted//\"/}"
  if [ -n "${ORTUS_DEPS_CACHE:-}" ]; then
    dir="$ORTUS_DEPS_CACHE/$name-$rev"
    mkdir -p "$ORTUS_DEPS_CACHE"
  else
    dir=".deps/$name"
    [ -L "$dir" ] && rm -f "$dir"
  fi
  # Several repositories may fetch into the cache at once.
  exec 9>"$dir.lock" 2>/dev/null || exec 9>/dev/null
  flock 9 2>/dev/null || true
  if [ ! -d "$dir/.git" ]; then
    git init -q "$dir"
    git -C "$dir" remote add origin "$org/$name.git"
  fi
  if [ "$(git -C "$dir" rev-parse -q --verify HEAD 2>/dev/null || true)" != "$rev" ]; then
    git -C "$dir" fetch -q --depth 1 origin "$rev"
    git -C "$dir" checkout -q --force "$rev"
    rm -f "$dir/.web-built"
  fi
  if [ -f "$dir/web/package.json" ] && [ ! -f "$dir/.web-built" ]; then
    (cd "$dir/web" && npm ci --silent && npm run build --silent)
    touch "$dir/.web-built"
  fi
  exec 9>&-
  if [ -n "${ORTUS_DEPS_CACHE:-}" ]; then
    if [ -d ".deps/$name" ] && [ ! -L ".deps/$name" ]; then rm -rf ".deps/$name"; fi
    ln -sfn "$dir" ".deps/$name"
  fi
  echo "fetch: $name at ${rev:0:12}"
done
