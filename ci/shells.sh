#!/bin/sh
# Each shell runs a command, then plain `uhh`, and must explain that command.
set -u
status=0
check() {
    shell=$1
    script=$2
    shift 2
    out=$(printf '%s\n' "$script" | NO_COLOR=1 HISTFILE=/dev/null "$@" 2>&1)
    if printf '%s\n' "$out" | grep -q 'uhh:  ls -la /tmp'; then
        echo "ok    $shell"
    else
        echo "FAIL  $shell"
        printf '%s\n' "$out" | sed 's/^/      /'
        status=1
    fi
}
check_shell() {
    name=$1
    shift
    if ! command -v "$name" >/dev/null 2>&1; then
        if [ -n "${UHH_REQUIRE_ALL:-}" ]; then
            echo "FAIL  $name is not installed"
            status=1
        else
            echo "skip  $name"
        fi
        return
    fi
    check "$name" "$@"
}
script_posix='eval "$(uhh --init SHELL)"
set -o history 2>/dev/null
ls -la /tmp
uhh'
check_shell bash "$(echo "$script_posix" | sed s/SHELL/bash/)" bash --norc -i
check_shell zsh "$(echo "$script_posix" | sed s/SHELL/zsh/)" zsh -f -i
check_shell fish 'uhh --init fish | source
ls -la /tmp
uhh' fish --no-config -i
exit $status
