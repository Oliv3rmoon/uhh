//! The "heads up" lines: things in a command that can't be taken back.

use std::path::Path;

use crate::explain::{Cmd, Explained};

const SHELLS: [&str; 11] = [
    "sh", "bash", "zsh", "fish", "dash", "ksh", "python", "python3", "perl", "ruby", "node",
];

/// Where `rm -rf` means everything.
const EVERYTHING: [&str; 9] = ["/", "/*", "~", "~/", "~/*", "*", ".", "..", "$HOME"];

pub fn warnings(line: &str, explained: &Explained) -> Vec<String> {
    let mut out = Vec::new();
    if line.replace(' ', "").contains(":(){:|:&};:") {
        out.push(
            "this is a fork bomb: it copies itself over and over until the machine freezes.".into(),
        );
    }
    let commands = &explained.commands;
    for (index, cmd) in commands.iter().enumerate() {
        // `-rf` counts as `-r` and `-f` even when there was no manual to
        // split it with.
        let has = |names: &[&str]| {
            cmd.flags.iter().any(|f| {
                names.contains(&f.as_str())
                    || (!f.starts_with("--")
                        && f.chars()
                            .skip(1)
                            .any(|c| names.contains(&format!("-{c}").as_str())))
            })
        };
        let sub = cmd
            .subcommand
            .as_deref()
            .or(cmd.args.first().map(String::as_str))
            .unwrap_or("");
        match cmd.program.as_str() {
            "sudo" | "doas" => out.push(format!(
                "{} runs it as root, the admin account, so it can change anything on this machine.",
                cmd.program
            )),
            "rm" if has(&["-r", "-R", "--recursive"]) && has(&["-f", "--force"]) => {
                let targets: Vec<&str> = cmd.args.iter().map(String::as_str).collect();
                if let Some(t) = targets.iter().find(|t| EVERYTHING.contains(t)) {
                    out.push(format!(
                        "rm -rf on {t} deletes everything there. no trash, no undo, no asking."
                    ));
                } else {
                    out.push(format!(
                        "deletes {} and everything inside, without asking. there's no trash to get it back from.",
                        list(&targets)
                    ));
                }
            }
            "dd" => {
                if let Some(dev) = cmd.args.iter().find_map(|a| a.strip_prefix("of=/dev/")) {
                    out.push(format!(
                        "dd writes straight onto /dev/{dev}. if that's the wrong disk, it's wiped."
                    ));
                }
            }
            p if p.starts_with("mkfs") => out.push(format!(
                "formats {}: everything on it is erased.",
                list(&cmd.args.iter().map(String::as_str).collect::<Vec<_>>())
            )),
            "diskutil" if cmd.args.first().is_some_and(|a| a.starts_with("erase")) => {
                out.push("erases a disk or volume: everything on it is gone.".into());
            }
            "chmod" if cmd.args.iter().any(|a| matches!(a.as_str(), "777" | "a+rwx" | "o+w")) => {
                out.push("that lets every user on this machine read, change and run it.".into());
            }
            "git" if sub == "push" && has(&["-f", "--force"]) => out.push(
                "--force overwrites the remote branch, and commits other people pushed can vanish. --force-with-lease is the safer way."
                    .into(),
            ),
            "git" if sub == "reset" && has(&["--hard"]) => {
                out.push("--hard throws away your uncommitted changes, for good.".into());
            }
            "git" if sub == "clean" && has(&["-f", "--force"]) => out.push(
                "git clean -f deletes untracked files for good. run it with -n first to see what goes."
                    .into(),
            ),
            "git" if matches!(sub, "checkout" | "restore") && cmd.args.iter().any(|a| a == ".") => {
                out.push("that throws away your unsaved changes to every file here.".into());
            }
            p if SHELLS.contains(&p) && cmd.piped_in && downloads_before(commands, index) => {
                out.push(format!(
                    "this runs a script from the internet without letting you read it first. drop the `|\u{a0}{p}` to see what it does."
                ));
            }
            _ => {}
        }
        for (op, target) in &cmd.redirects {
            if target.starts_with("/dev/disk")
                || target.starts_with("/dev/sd")
                || target.starts_with("/dev/nvme")
            {
                out.push(format!("that writes straight onto the disk {target}."));
            } else if matches!(op.as_str(), ">" | "1>" | "&>") && Path::new(target).is_file() {
                out.push(format!(
                    "{target} already exists, and `{op}` replaces what's in it. `>>` adds to the end instead."
                ));
            }
        }
    }
    out.dedup();
    out
}

fn downloads_before(commands: &[Cmd], index: usize) -> bool {
    commands[..index].iter().any(|c| {
        matches!(
            c.program.as_str(),
            "curl" | "wget" | "fetch" | "http" | "iwr"
        )
    })
}

fn list(items: &[&str]) -> String {
    match items {
        [] => "it".into(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explain::{Manuals, explain};

    fn heads_up(line: &str) -> Vec<String> {
        warnings(line, &explain(line, &mut Manuals::default()))
    }

    #[test]
    fn curl_into_a_shell() {
        let w = heads_up("curl -fsSL https://example.com/install.sh | sudo bash");
        assert!(w.iter().any(|w| w.starts_with("sudo runs it as root")));
        assert!(w.iter().any(|w| w.contains("drop the `|\u{a0}bash`")));
        assert!(heads_up("curl https://x | jq .").is_empty());
    }

    #[test]
    fn rm_rf() {
        let w = heads_up("rm -rf node_modules dist");
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("deletes node_modules and dist"));
        assert!(heads_up("rm -rf /")[0].starts_with("rm -rf on /"));
        assert!(heads_up("rm -r dir").is_empty());
    }

    #[test]
    fn git_and_disks() {
        assert_eq!(heads_up("git push --force origin main").len(), 1);
        assert!(heads_up("git push --force-with-lease").is_empty());
        assert_eq!(heads_up("git reset --hard HEAD~1").len(), 1);
        assert_eq!(heads_up("sudo dd if=x.img of=/dev/disk4 bs=4m").len(), 2);
        assert_eq!(heads_up(":(){ :|:& };:").len(), 1);
        assert_eq!(heads_up("chmod -R 777 .").len(), 1);
    }
}
