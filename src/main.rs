mod danger;
mod explain;
mod lex;
mod manual;
mod render;

use std::env;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};

use crate::render::Paint;

/// Explains a shell command, flag by flag, from the manuals on your machine.
#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "examples:\n  uhh tar -xzvf backup.tgz\n  uhh 'curl -fsSL https://example.com/install.sh | sh'\n  uhh                  explains your last command, once `uhh --init zsh` is set up"
)]
struct Cli {
    /// Show each part's whole manual entry, not just the start.
    #[arg(long)]
    full: bool,
    /// Print the shell setup that makes plain `uhh` explain your last
    /// command. Add `eval "$(uhh --init zsh)"` to your shell config.
    #[arg(long, value_name = "SHELL")]
    init: Option<Shell>,
    /// Explain the last command in UHH_HISTORY (the shell setup uses this).
    #[arg(long, hide = true)]
    last: bool,
    /// The command. Quote it if it has `|`, `>` or `&&` in it.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Bash,
    Zsh,
    Fish,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let paint = Paint { color: color() };
    if let Some(shell) = cli.init {
        print!("{}", init(shell));
        return ExitCode::SUCCESS;
    }
    let line = if cli.last {
        last_command()
    } else if cli.command.len() == 1 {
        Some(cli.command[0].clone())
    } else if cli.command.is_empty() {
        None
    } else {
        Some(shell_words::join(&cli.command))
    };
    let Some(line) = line.filter(|l| !l.trim().is_empty()) else {
        let hint = if cli.last {
            "no command before this one to explain."
        } else {
            "uhh what? give me a command: uhh tar -xzvf backup.tgz"
        };
        eprintln!("{}", paint.line(hint));
        return ExitCode::from(2);
    };

    let mut manuals = explain::Manuals::default();
    let explained = explain::explain(&line, &mut manuals);
    let warnings = danger::warnings(&line, &explained);
    let text = render::render(&line, &explained, &warnings, paint, width(), cli.full);
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(text.as_bytes());
    ExitCode::SUCCESS
}

/// Colours when printing to a terminal, unless `NO_COLOR` is set;
/// `CLICOLOR_FORCE` turns them on anyway.
fn color() -> bool {
    let set = |name| env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0");
    if set("NO_COLOR") {
        return false;
    }
    set("CLICOLOR_FORCE") || std::io::stdout().is_terminal()
}

fn width() -> usize {
    let columns = crossterm::terminal::size()
        .ok()
        .map(|(w, _)| w as usize)
        .filter(|_| std::io::stdout().is_terminal())
        .or_else(|| env::var("COLUMNS").ok()?.parse().ok())
        .unwrap_or(100);
    columns.clamp(40, 120)
}

/// The newest line in UHH_HISTORY that isn't `uhh` itself.
fn last_command() -> Option<String> {
    let history = env::var("UHH_HISTORY").ok()?;
    let mut lines: Vec<&str> = history.lines().map(str::trim).collect();
    if env::var_os("UHH_NEWEST_FIRST").is_some() {
        lines.reverse();
    }
    lines
        .into_iter()
        .rev()
        .find(|line| !line.is_empty() && line.split_whitespace().next() != Some("uhh"))
        .map(str::to_string)
}

fn init(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash | Shell::Zsh => {
            r#"uhh() {
    if [ "$#" -eq 0 ]; then
        UHH_HISTORY="$(fc -ln -10)" command uhh --last
    else
        command uhh "$@"
    fi
}
"#
        }
        Shell::Fish => {
            r#"function uhh --description 'Explain a command, or the last one'
    if test (count $argv) -eq 0
        env UHH_NEWEST_FIRST=1 UHH_HISTORY=(history --max 10 | string collect) uhh --last
    else
        command uhh $argv
    end
end
"#
        }
    }
}
