#!/bin/sh
# Builds the throwaway folder the demo runs in: a repo with some history,
# a backup.tgz to not defuse, and a zsh set up with uhh.
set -e
root="$1"
rm -rf "$root" && mkdir -p "$root/zdotdir" "$root/project"
cd "$root/project"
git init -q -b main
git config user.name demo && git config user.email demo@example.com
for msg in "first commit" "add the parser" "fix the parser" "fix the fix" "it works, don't touch it" "touch it"; do
    echo "$msg" >> notes.txt && git add notes.txt && git commit -qm "$msg"
done
tar -czf backup.tgz notes.txt
cat > "$root/zdotdir/.zshrc" <<'RC'
PROMPT='%F{cyan}%1~%f %F{green}❯%f '
HISTSIZE=100
setopt interactivecomments
unsetopt beep
bindkey -e
PROMPT_EOL_MARK=''
eval "$(uhh --init zsh)"
RC
