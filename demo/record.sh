#!/bin/sh
# Opens the demo inside the terminal it is started from. Usage:
#   demo/record.sh [go-file]
# With a go-file it waits for that file to appear before typing, so a screen
# recording can be started first.
set -e
here="$(cd "$(dirname "$0")" && pwd)"
root=/tmp/uhh-demo
sh "$here/setup.sh" "$root"
export PATH="$here/../target/release:$PATH" ZDOTDIR="$root/zdotdir" GIT_CONFIG_GLOBAL=/dev/null TERM=xterm-256color
cd "$root/project"
printf '\033[H\033[2J'
if [ -n "$1" ]; then while [ ! -f "$1" ]; do sleep 0.1; done; fi
expect ${UHH_DEMO_DEBUG:+-d} "$here/demo.exp"
