#!/bin/sh
# Runs ci/shell.exp for every shell that is installed. Fails if one fails.
set -e
here="$(cd "$(dirname "$0")" && pwd)"
export NO_COLOR=1 HISTFILE=/dev/null
failed=0
for shell in bash zsh fish; do
    if command -v "$shell" >/dev/null; then
        expect "$here/shell.exp" "$shell" || failed=1
    elif [ -n "$UHH_REQUIRE_ALL" ]; then
        echo "== $shell: not installed"
        failed=1
    else
        echo "== $shell: not installed, skipped"
    fi
done
exit $failed
