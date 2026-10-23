//! Reads a program's manual (its man page, or `--help` for the tools that
//! only have that) and finds the paragraph for each option in it.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Tools with no man page whose `--help` is safe to run and lists options.
const HELP_ONLY: [&str; 30] = [
    "docker",
    "podman",
    "kubectl",
    "helm",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "deno",
    "node",
    "cargo",
    "rustup",
    "go",
    "gh",
    "pip",
    "pip3",
    "uv",
    "poetry",
    "terraform",
    "aws",
    "gcloud",
    "az",
    "flyctl",
    "vercel",
    "netlify",
    "wrangler",
    "supabase",
    "railway",
    "brew",
    "rustc",
];

pub fn is_help_only(program: &str) -> bool {
    HELP_ONLY.contains(&program)
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// What the option takes (`file`, `<msg>`), if anything.
    pub arg: Option<String>,
    /// `[=WHEN]`: the value is optional and never the next word.
    pub optional: bool,
    /// The paragraphs that describe it.
    pub text: Vec<String>,
}

impl Entry {
    pub fn takes_arg(&self) -> bool {
        self.arg.is_some() && !self.optional
    }
}

#[derive(Debug, Default)]
pub struct Page {
    /// The one-line summary from NAME, or the first line of `--help`.
    pub summary: Option<String>,
    pub entries: Vec<Entry>,
    /// What the plain arguments are called, in order.
    pub arguments: Vec<String>,
    /// The last of `arguments` can be given again and again.
    pub repeats: bool,
    lines: Vec<String>,
    index: HashMap<String, usize>,
}

impl Page {
    /// The man page for `name`, or its `--help` if it's one of the tools
    /// that only has that. `words` are subcommands (`git commit` reads
    /// git-commit's page; `docker run` reads `docker run --help`).
    pub fn load(name: &str, words: &[&str]) -> Option<Page> {
        let man_name = std::iter::once(name)
            .chain(words.iter().copied())
            .collect::<Vec<_>>()
            .join("-");
        if let Some(text) = man(&man_name) {
            return Some(Page::parse(&text));
        }
        let program = name.rsplit('/').next().unwrap_or(name);
        if HELP_ONLY.contains(&program) {
            let mut args: Vec<&str> = words.to_vec();
            args.push("--help");
            let text = run(name, &args)?;
            return Some(Page::parse(&text));
        }
        None
    }

    pub fn parse(text: &str) -> Page {
        let lines: Vec<String> = clean(text).lines().map(str::to_string).collect();
        let (arguments, repeats) = synopsis(&lines);
        let mut page = Page {
            summary: summary(&lines),
            arguments,
            repeats,
            ..Page::default()
        };
        let mut i = 0;
        while i < lines.len() {
            let Some((indent, header, rest)) = parse_header(&lines[i]) else {
                i += 1;
                continue;
            };
            let (mut names, mut arg, mut optional) = names_of(header);
            // Several forms of one option on their own lines (`ssh -L`).
            let mut j = i + 1;
            let mut text = Vec::new();
            // Text on the header's own line starts the first paragraph.
            let mut first = rest.to_string();
            if rest.is_empty() {
                while let Some((more_indent, more, more_rest)) =
                    lines.get(j).and_then(|line| parse_header(line))
                {
                    if more_indent != indent {
                        break;
                    }
                    let (more_names, more_arg, more_optional) = names_of(more);
                    for name in more_names {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                    if arg.is_none() {
                        arg = more_arg;
                        optional = more_optional;
                    }
                    j += 1;
                    if !more_rest.is_empty() {
                        first = more_rest.to_string();
                        break;
                    }
                }
            }
            j = body(&lines, j, indent, &mut text, first);
            if !text.is_empty() {
                for name in &names {
                    page.index.entry(name.clone()).or_insert(page.entries.len());
                }
                page.entries.push(Entry {
                    arg,
                    optional,
                    text,
                });
            }
            i = j.max(i + 1);
        }
        page.lines = lines;
        page
    }

    /// What the `n`th plain argument is called.
    pub fn argument(&self, n: usize) -> Option<&str> {
        match self.arguments.get(n) {
            Some(name) => Some(name),
            None if self.repeats => self.arguments.last().map(String::as_str),
            None => None,
        }
    }

    pub fn option(&self, name: &str) -> Option<&Entry> {
        self.index.get(name).map(|&i| &self.entries[i])
    }

    /// `--verb` for `--verbose`, when only one long option starts that way.
    pub fn option_prefix(&self, prefix: &str) -> Option<(&str, &Entry)> {
        if !prefix.starts_with("--") || prefix.len() < 4 {
            return None;
        }
        let mut found = self
            .index
            .iter()
            .filter(|(name, _)| name.starts_with(prefix));
        let first = found.next()?;
        let same = |(_, &i): (&String, &usize)| i == *first.1;
        if found.all(same) {
            Some((first.0.as_str(), &self.entries[*first.1]))
        } else {
            None
        }
    }

    /// The description of a subcommand listed in this page (`brew install`,
    /// `cargo build`): a line that starts with the word, then its text.
    pub fn word(&self, word: &str) -> Option<String> {
        for (i, line) in self.lines.iter().enumerate() {
            let indent = indent_of(line);
            let trimmed = line.trim_start();
            let first = trimmed.split_whitespace().next().unwrap_or("");
            if first.trim_end_matches(',') != word || indent > 12 {
                continue;
            }
            let after = &trimmed[first.len()..];
            if let Some(gap) = after.find("  ") {
                let rest = after[gap..].trim();
                // `build, b    Compile ...`: aliases sit before the gap.
                if !rest.is_empty() && !rest.starts_with('-') {
                    return Some(rest.to_string());
                }
            }
            let mut text = Vec::new();
            body(&self.lines, i + 1, indent, &mut text, String::new());
            if let Some(first) = text.into_iter().next() {
                return Some(first);
            }
        }
        None
    }
}

/// Collects the paragraphs indented under a header at `indent`, starting at
/// line `j`, and returns the line after them.
fn body(
    lines: &[String],
    mut j: usize,
    indent: usize,
    text: &mut Vec<String>,
    mut paragraph: String,
) -> usize {
    while j < lines.len() {
        let line = &lines[j];
        if line.trim().is_empty() {
            if !paragraph.is_empty() {
                text.push(std::mem::take(&mut paragraph));
            }
            // Keep going only if the next text is still indented under it.
            let next = lines[j..].iter().find(|l| !l.trim().is_empty());
            if next.is_none_or(|l| indent_of(l) <= indent || parse_header(l).is_some()) {
                break;
            }
            j += 1;
            continue;
        }
        // The next option, even when `--help` indents it deeper.
        if indent_of(line) <= indent || parse_header(line).is_some() {
            break;
        }
        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(line.trim());
        j += 1;
    }
    if !paragraph.is_empty() {
        text.push(paragraph);
    }
    j
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// An option header: `-f file, --file file` with maybe its text after a
/// gap of two spaces. Returns the indent, the header and the text.
fn parse_header(line: &str) -> Option<(usize, &str, &str)> {
    let indent = indent_of(line);
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    let second = chars.next()?;
    if indent > 16 || !(first == '-' || first == '+') {
        return None;
    }
    if !(second.is_ascii_alphanumeric() || matches!(second, '-' | '?' | '#' | '@' | '<')) {
        return None;
    }
    match trimmed.find("  ") {
        Some(gap) => Some((indent, trimmed[..gap].trim_end(), trimmed[gap..].trim())),
        None => Some((indent, trimmed.trim_end(), "")),
    }
}

/// `-A num, --after-context=num` → names and the argument they share.
fn names_of(header: &str) -> (Vec<String>, Option<String>, bool) {
    let mut names = Vec::new();
    let mut arg = None;
    let mut optional = false;
    for piece in header.split(", ").flat_map(|p| p.split(" | ")) {
        let piece = piece.trim();
        if !(piece.starts_with('-') || piece.starts_with('+')) {
            continue;
        }
        let end = piece
            .char_indices()
            .skip(1)
            .find(|&(_, c)| {
                !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '?' | '#' | '@'))
            })
            .map_or(piece.len(), |(i, _)| i);
        let name = piece[..end].trim_end_matches('.');
        if name.len() < 2 {
            continue;
        }
        names.push(name.to_string());
        let rest = piece[end..].trim_start_matches("...");
        if arg.is_none() {
            let value = rest.trim_start_matches(['=', ' ']).trim();
            let bracket = rest.starts_with('[');
            let value = value.trim_start_matches("[=").trim_start_matches('=');
            if !value.is_empty() && !value.starts_with('-') {
                optional = bracket;
                arg = Some(
                    value
                        .trim_end_matches(']')
                        .trim_end_matches("...")
                        .to_string(),
                );
            }
        }
    }
    (names, arg, optional)
}

fn summary(lines: &[String]) -> Option<String> {
    if let Some(at) = lines.iter().position(|l| l.trim() == "NAME") {
        let mut text = String::new();
        for line in &lines[at + 1..] {
            if line.trim().is_empty() {
                if text.is_empty() {
                    continue;
                }
                break;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(line.trim());
        }
        for dash in [" - ", " – ", " — ", " -- "] {
            if let Some(i) = text.find(dash) {
                return Some(text[i + dash.len()..].trim().to_string());
            }
        }
        return (!text.is_empty()).then_some(text);
    }
    // `--help`: the first paragraph that isn't usage.
    let mut paragraphs = lines
        .split(|l| l.trim().is_empty())
        .filter(|p| !p.is_empty());
    let first = paragraphs.find(|p| {
        let l = p[0].trim();
        !l.to_lowercase().starts_with("usage") && !l.starts_with('-') && !l.ends_with(':')
    })?;
    Some(first.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" "))
}

/// The names of the plain arguments, from SYNOPSIS or `Usage:`:
/// `grep [-i] [pattern] [file ...]` gives `pattern`, `file` (repeating).
fn synopsis(lines: &[String]) -> (Vec<String>, bool) {
    let mut text = String::new();
    if let Some(at) = lines.iter().position(|l| l.trim() == "SYNOPSIS") {
        let mut first_indent = None;
        for line in lines[at + 1..].iter().skip_while(|l| l.trim().is_empty()) {
            let indent = indent_of(line);
            match first_indent {
                None => first_indent = Some(indent),
                Some(first) if line.trim().is_empty() || indent <= first => break,
                _ => {}
            }
            text.push(' ');
            text.push_str(line.trim());
        }
    } else if let Some(line) = lines
        .iter()
        .map(|l| l.trim())
        .find(|l| l.to_lowercase().starts_with("usage:"))
    {
        text = line[6..].to_string();
    }
    let mut groups = Vec::new();
    let mut depth = 0;
    let mut group = String::new();
    for c in text.chars() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                if !group.is_empty() {
                    groups.push(std::mem::take(&mut group));
                }
                continue;
            }
            _ => {}
        }
        group.push(c);
    }
    if !group.is_empty() {
        groups.push(group);
    }
    let mut names: Vec<String> = Vec::new();
    let mut repeats = false;
    let mut leading = true;
    for group in groups {
        let inner = group.trim_matches(['[', ']', '{', '}']).trim();
        let bracketed = group.starts_with('[');
        // `git commit [...]`: the program's own words come first.
        if leading
            && !bracketed
            && names.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
        {
            continue;
        }
        leading = false;
        if inner.starts_with('-') || inner.is_empty() {
            continue;
        }
        // `[<repository> [<refspec>...]]` names two; `<file> | <pattern>`
        // is one with two names.
        let flat = inner.replace(['[', ']', '{', '}'], " ");
        // `[bundled-flags <args>]`, `[OPTIONS]`: options, not arguments.
        let head = flat.split_whitespace().next().unwrap_or("").to_lowercase();
        if head.contains("option") || head.contains("flag") {
            continue;
        }
        let mut or = false;
        for word in flat.split_whitespace() {
            if word == "|" {
                or = true;
                continue;
            }
            repeats = word.ends_with("...");
            let word = word.trim_end_matches("...").trim_matches(['<', '>', '.']);
            if word.is_empty() || word.starts_with('-') || word.starts_with('=') {
                or = false;
                continue;
            }
            let lower = word.to_lowercase();
            if lower.contains("option") || lower.contains("flag") {
                continue;
            }
            let word = if word == word.to_uppercase() {
                lower
            } else {
                word.to_string()
            };
            let word = word.replace('|', " or ");
            match names.last_mut() {
                Some(last) if or => *last = format!("{last} or {word}"),
                _ => names.push(word),
            }
            or = false;
        }
        if group.contains("...") {
            repeats = true;
        }
    }
    (names, repeats)
}

/// Drops terminal formatting: `X\bX` bold, `_\bX` underline and colours.
fn clean(text: &str) -> String {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x08' => {
                out.pop();
            }
            '\x1b' => {
                if chars.peek() == Some(&'[') {
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            }
            '\u{2010}' | '\u{2212}' => out.push('-'),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.into_iter().collect()
}

/// The man page as plain text, one paragraph per line.
fn man(name: &str) -> Option<String> {
    if name.contains('/') || name.starts_with('-') {
        return None;
    }
    let mut command = Command::new("man");
    command
        .arg(name)
        .env("MANWIDTH", "1000")
        .env("MANPAGER", "cat")
        .env("PAGER", "cat")
        .env("GROFF_NO_SGR", "1")
        .env_remove("MAN_KEEP_FORMATTING");
    let text = capture(command)?;
    // On macOS `man cd` is the page for every shell builtin at once.
    let page = Page::parse(&text);
    if page
        .summary
        .as_deref()
        .is_some_and(|s| s.starts_with("builtin,") || s.contains("shell built-in commands"))
    {
        return None;
    }
    (text.lines().count() > 3).then_some(text)
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args).env("NO_COLOR", "1").env("TERM", "dumb");
    capture(command)
}

/// Runs it with no input, gives up after 3 seconds, and returns what it
/// printed if that looks like something.
fn capture(mut command: Command) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let text = String::from_utf8_lossy(&reader.join().ok()?).into_owned();
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BSD: &str = "TAR(1)    General Commands Manual    TAR(1)

NAME
     tar - manipulate tape archives

DESCRIPTION
     -x      Extract to disk from the archive.  The long option form is --extract.

     -f file, --file file
             Read the archive from or write the archive to the specified file.

             The default varies by system.

     -z, --gunzip, --gzip
             (c mode only) Compress the resulting archive with gzip(1).

     -L [bind_address:]port:host:hostport
     -L local_socket:remote_socket
             Specifies that connections to the given port are to be forwarded.
";

    const GNU: &str = "GREP(1)

NAME
       grep, egrep - print lines that match patterns

OPTIONS
       -A NUM, --after-context=NUM
              Print NUM lines of trailing context after matching lines.

       --color[=WHEN], --colour[=WHEN]
              Surround the matched strings with escape sequences.

       -i, --ignore-case
              Ignore case distinctions in patterns and input data.
";

    const HELP: &str = "Rust's package manager

Usage: cargo [+toolchain] [OPTIONS] [COMMAND]

Options:
  -V, --version                  Print version info and exit
  -v, --verbose...               Use verbose output (-vv very verbose/build.rs output)
  -C <DIRECTORY>                 Change to DIRECTORY before doing anything (nightly-only)
  -Z <FLAG>                      Unstable (nightly-only) flags to Cargo, see 'cargo -Z help' for
                                 details

Commands:
    build, b    Compile the current package
    new         Create a new cargo package
";

    #[test]
    fn bsd_pages() {
        let page = Page::parse(BSD);
        assert_eq!(page.summary.as_deref(), Some("manipulate tape archives"));
        assert!(page.option("-x").unwrap().text[0].starts_with("Extract to disk"));
        let f = page.option("--file").unwrap();
        assert!(std::ptr::eq(f, page.option("-f").unwrap()));
        assert_eq!(f.arg.as_deref(), Some("file"));
        assert!(f.takes_arg());
        assert_eq!(f.text.len(), 2);
        assert!(!page.option("-z").unwrap().takes_arg());
        let l = page.option("-L").unwrap();
        assert!(l.takes_arg() && l.text[0].starts_with("Specifies"));
    }

    #[test]
    fn gnu_pages() {
        let page = Page::parse(GNU);
        assert_eq!(
            page.summary.as_deref(),
            Some("print lines that match patterns")
        );
        assert_eq!(page.option("-A").unwrap().arg.as_deref(), Some("NUM"));
        let color = page.option("--colour").unwrap();
        assert!(color.optional && !color.takes_arg());
        assert!(page.option("-i").unwrap().text[0].starts_with("Ignore case"));
        assert_eq!(page.option_prefix("--ignore").unwrap().0, "--ignore-case");
        assert!(page.option_prefix("--co").is_some(), "both are one option");
    }

    #[test]
    fn help_text() {
        let page = Page::parse(HELP);
        assert_eq!(page.summary.as_deref(), Some("Rust's package manager"));
        assert!(page.option("--verbose").is_some());
        assert_eq!(
            page.option("-C").unwrap().arg.as_deref(),
            Some("<DIRECTORY>")
        );
        assert!(page.option("-Z").unwrap().text[0].ends_with("for details"));
        assert_eq!(
            page.word("build").as_deref(),
            Some("Compile the current package")
        );
        assert_eq!(
            page.word("new").as_deref(),
            Some("Create a new cargo package")
        );
        assert_eq!(page.word("b"), None);
    }

    #[test]
    fn argument_names() {
        let page = Page::parse(
            "NAME\n     grep - search\n\nSYNOPSIS\n     grep [-abc] [-A num] [--color[=when]] [pattern] [file ...]\n\nDESCRIPTION\n",
        );
        assert_eq!(page.arguments, ["pattern", "file"]);
        assert_eq!(page.argument(5), Some("file"));
        let page = Page::parse(
            "SYNOPSIS\n       git commit [-a | --interactive] [-m <msg>]\n                  [--] [<pathspec>...]\n\nX\n",
        );
        assert_eq!(page.arguments, ["pathspec"]);
        let page = Page::parse(
            "     -<number>, -n <number>, --max-count=<number>\n         Limit the number of commits.\n",
        );
        assert_eq!(page.option("-n").unwrap().arg.as_deref(), Some("<number>"));
        let page = Page::parse("Usage:  docker run [OPTIONS] IMAGE [COMMAND] [ARG...]\n");
        assert_eq!(page.arguments, ["image", "command", "arg"]);
        let page = Page::parse("Usage: brew install [options] formula|cask [...]\n");
        assert_eq!(page.arguments, ["formula or cask"]);
        let page = Page::parse(
            "SYNOPSIS\n     ssh [-46] [-B bind_interface] destination [command [argument ...]]\n",
        );
        assert_eq!(page.arguments, ["destination", "command", "argument"]);
        let page = Page::parse("SYNOPSIS\n       git push [--all] [<repository> [<refspec>...]]\n");
        assert_eq!(page.arguments, ["repository", "refspec"]);
        assert_eq!(page.argument(3), Some("refspec"));
        let page =
            Page::parse("SYNOPSIS\n     tar [bundled-flags <args>] [<file> | <pattern> ...]\n");
        assert_eq!(page.arguments, ["file or pattern"]);
    }

    #[test]
    fn strips_overstrike() {
        assert_eq!(clean("-\x08-x\x08x  _\x08f_\x08i"), "-x  fi");
    }
}
