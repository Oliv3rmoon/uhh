<p align="center"><img src="docs/logo.svg" alt="uhh" width="400"></p>

<p align="center"><b>Not sure what a command does? Type <code>uhh</code> in front of it.</b><br>
It explains every flag, pipe and redirect, straight from the man pages already on your machine.<br>
<a href="https://explainshell.com">explainshell</a> for your terminal: offline, instant, and it warns you before you paste something you'll regret.</p>

<p align="center">
<a href="https://crates.io/crates/uhh"><img src="https://img.shields.io/crates/v/uhh?color=7DD3FC&labelColor=141414" alt="crates.io"></a>
<a href="https://buymeacoffee.com/oliv3rmoon"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-FFDD00?style=flat&logo=buymeacoffee&logoColor=black" alt="Buy me a coffee"></a>
<a href="https://discord.gg/pSqgRU3YP9"><img src="https://img.shields.io/badge/Discord-The%20Commons-5865F2?style=flat&logo=discord&logoColor=white" alt="Discord"></a>
</p>

<p align="center"><img src="docs/demo.gif" alt="uhh explaining a curl | sudo bash install script and warning about it, defusing the xkcd tar bomb with tar -xzvf, explaining the git log command run just before it, walking through a fork bomb, and explaining itself" width="900"></p>

<p align="center"><sub>Also as <a href="docs/demo.mp4">an MP4</a>.</sub></p>

```console
$ uhh tar -xzvf backup.tgz
 uhh  tar -xzvf backup.tgz

  tar              manipulate tape archives
    -x             Extract to disk from the archive.
    -z             (c mode only) Compress the resulting archive with gzip(1).
    -v             Produce verbose output. In create and extract modes, tar will list each file name as it is read from
                   or written to the archive.
    -f backup.tgz  Read the archive from or write the archive to the specified file.
```

In your terminal, each part of the command is coloured, and its line underneath is the same colour, so you can see what goes with what.

## Before you paste that install script

```console
$ uhh 'curl -fsSL https://example.com/install.sh | sudo bash'
 uhh  curl -fsSL https://example.com/install.sh | sudo bash

  curl  transfer a URL
    -f  (HTTP) Fail fast with no output at all on server errors.
    -s  Silent or quiet mode. Do not show progress meter or error messages.
    -S  When used with -s, --silent, it makes curl show an error message if it fails.
    -L  (HTTP) If the server reports that the requested page has moved to a different location (indicated with a
        Location: header and a 3XX response code), this option makes curl redo the request on the new place.
    https://example.com/install.sh
        a URL
  |     sends what curl prints into sudo
  sudo  execute a command as another user
  bash  GNU Bourne-Again SHell

  heads up  sudo runs it as root, the admin account, so it can change anything on this machine.
  heads up  this runs a script from the internet without letting you read it first. drop the `| bash` to see what it
            does.
```

uhh flags the commands you can't take back: `rm -rf`, `curl | sh`, `sudo`, `dd` onto a disk, `mkfs`, `chmod 777`, `git push --force`, `git reset --hard`, `git clean -f`, a `>` that would overwrite a file that exists, and fork bombs.

## Why uhh

explainshell is great, and it lives in a browser. uhh lives where the command is.

| | explainshell.com | uhh |
|---|---|---|
| Where | a website | your terminal |
| Works offline | no | yes |
| Manuals | the ones the site indexed | yours, for the exact versions you have installed (macOS and Linux tools differ, and uhh reads the right one) |
| Tools with no man page | no | `docker`, `cargo`, `kubectl`, `npm`, `gh` and friends, from their `--help` |
| Subcommands | some | `git commit`, `brew install`, `cargo build`, `docker compose up` |
| Warns before damage | no | yes |
| Your last command | copy, open a browser, paste | type `uhh` |

## What it understands

- Bundled flags: `-xzvf` is `-x -z -v -f`, and `-f` takes `backup.tgz`
- Old-style flags with no dash: `tar xzf`, `ps aux`
- Long options, `--name=value`, and abbreviations (`--verb` for `--verbose`)
- Values: `-m "msg"`, `-n5`, `-L 8080:localhost:80`
- Arguments by name, from the manual's synopsis: `TODO` is *the pattern* for grep, `user@host` is *the destination* for ssh
- Files and folders that exist, URLs, globs, `$VARIABLES` (with their value here), `$(command substitution)`, `~`
- Pipes, `&&`, `||`, `;`, `&`, subshells, and every redirect: `>`, `>>`, `2>&1`, `&>`, `<`, `<<<`, `2>/dev/null`
- `sudo`, `env`, `xargs`, `time`, `nohup` and other commands that run a command
- `find -exec ... \;`, `kill -9` (signal 9, KILL), `FOO=bar cmd`
- Shell builtins like `cd`, `export` and `source`
- Options that just point at another one: `-r  Equivalent to -R` shows what `-R` does

## Install

```sh
brew install oliv3rmoon/tap/uhh
```

or `cargo install uhh`, or grab a build for macOS or Linux from the [releases](https://github.com/Oliv3rmoon/uhh/releases).

Quote the command when it has a `|`, `>`, `&&` or `;` in it, so your shell hands the whole thing to uhh:

```sh
uhh 'ps aux | grep node | awk "{print \$2}" | xargs kill -9'
```

### Explain the command you just ran

Add one line to your shell config, and plain `uhh` explains the command before it:

```sh
eval "$(uhh --init zsh)"          # ~/.zshrc
eval "$(uhh --init bash)"         # ~/.bashrc
uhh --init fish | source          # ~/.config/fish/config.fish
```

```console
$ git rebase -i --autosquash HEAD~3
$ uhh
```

`uhh --full` shows each option's whole manual entry instead of the start of it. `NO_COLOR=1` turns the colours off.

## Made to go with welp

[welp](https://github.com/Oliv3rmoon/welp) fixes the command you got wrong. uhh tells you what the one you got right does. When uhh meets a program you don't have, it'll point you to welp in case it's a typo.

## How it works

uhh splits the line the way your shell would, then reads each program's man page (`man` with the pager off) or, for tools without one, its `--help`. It finds every option in the page and reads the paragraph under it, then matches your flags against that list. Nothing leaves your machine, and it never runs the command you're asking about. The only thing it runs is `man`, or `--help` for a short list of well-known tools.

## License

MIT
