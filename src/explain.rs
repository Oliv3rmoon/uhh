//! Turns a command line into parts: each program, option, argument and
//! operator, with the words that explain it.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::lex::{self, Kind, Token};
use crate::manual::{Entry, Page, is_help_only};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Program,
    Sub,
    Flag,
    Arg,
    Op,
}

#[derive(Debug, Clone)]
pub struct Part {
    /// Where it sits in the line (an option and its value are two spans).
    pub spans: Vec<(usize, usize)>,
    pub label: String,
    pub text: String,
    /// The rest of its manual entry, for `--full`.
    pub more: Vec<String>,
    pub role: Role,
    /// Found in a manual, or in uhh's own notes.
    pub known: bool,
}

/// One command as run, for the warnings.
#[derive(Debug, Clone, Default)]
pub struct Cmd {
    pub program: String,
    pub subcommand: Option<String>,
    /// Options as their single names (`-xzf` is `-x`, `-z`, `-f`).
    pub flags: Vec<String>,
    pub args: Vec<String>,
    /// Reads what the command before it printed.
    pub piped_in: bool,
    pub redirects: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct Explained {
    pub parts: Vec<Part>,
    pub commands: Vec<Cmd>,
}

/// Programs that run the command after them.
const WRAPPERS: [&str; 16] = [
    "sudo",
    "doas",
    "env",
    "nohup",
    "nice",
    "time",
    "timeout",
    "exec",
    "command",
    "builtin",
    "xargs",
    "watch",
    "stdbuf",
    "unbuffer",
    "caffeinate",
    "strace",
];

/// Old-style options with no dash: `tar xzf`, `ps aux`.
const DASHLESS: [&str; 3] = ["tar", "ps", "ar"];

const BUILTINS: [(&str, &str); 26] = [
    ("cd", "change the current folder"),
    (
        "export",
        "set an environment variable for this shell and everything it runs",
    ),
    (
        "source",
        "run a file's commands in this shell, so its settings stick",
    ),
    (
        ".",
        "run a file's commands in this shell, so its settings stick",
    ),
    ("alias", "make a short name for a longer command"),
    ("unalias", "remove a short name made with alias"),
    ("eval", "run a string as a command"),
    ("exit", "close this shell"),
    ("set", "change shell options, or list shell variables"),
    ("unset", "remove a variable or function"),
    ("read", "read a line of input into a variable"),
    ("history", "list the commands you've run"),
    (
        "type",
        "say what a name is: a program, alias, builtin or function",
    ),
    ("pushd", "change folder and remember where you were"),
    ("popd", "go back to the folder pushd remembered"),
    ("jobs", "list what's running in the background"),
    ("fg", "bring a background job to the front"),
    ("bg", "keep a stopped job running in the background"),
    ("wait", "wait for background jobs to finish"),
    ("shift", "drop the first argument and move the rest down"),
    ("return", "leave the current function"),
    ("local", "make a variable that only exists in this function"),
    ("declare", "make a variable and set its type"),
    ("trap", "run something when the shell gets a signal"),
    ("umask", "set which permissions new files don't get"),
    ("ulimit", "show or set limits on what programs here can use"),
];

const SIGNALS: [(&str, &str, &str); 7] = [
    (
        "1",
        "HUP",
        "hang up: many servers reload their settings on it",
    ),
    ("2", "INT", "interrupt, the same as pressing ctrl-c"),
    ("3", "QUIT", "quit and dump core"),
    ("9", "KILL", "kill: stops it at once, no chance to clean up"),
    ("15", "TERM", "terminate: asks it to stop, the polite way"),
    ("18", "CONT", "continue a stopped process"),
    ("19", "STOP", "pause it"),
];

#[derive(Default)]
pub struct Manuals {
    pages: HashMap<String, Option<Rc<Page>>>,
}

impl Manuals {
    fn get(&mut self, program: &str, words: &[&str]) -> Option<Rc<Page>> {
        let key = std::iter::once(program)
            .chain(words.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        self.pages
            .entry(key)
            .or_insert_with(|| Page::load(program, words).map(Rc::new))
            .clone()
    }

    /// For tests: a page that's already been read.
    #[cfg(test)]
    pub fn insert(&mut self, key: &str, page: Page) {
        self.pages.insert(key.to_string(), Some(Rc::new(page)));
    }
}

pub fn explain(line: &str, manuals: &mut Manuals) -> Explained {
    if let Some(bomb) = fork_bomb(line) {
        return bomb;
    }
    let tokens = lex::lex(line);
    // Split at the operators between commands.
    let mut segments: Vec<(Vec<Token>, Option<Token>)> = Vec::new();
    let mut current = Vec::new();
    for token in tokens {
        if token.kind == Kind::Op && is_separator(&token.value) {
            segments.push((std::mem::take(&mut current), Some(token)));
        } else {
            current.push(token);
        }
    }
    if !current.is_empty() {
        segments.push((current, None));
    }
    let names: Vec<String> = segments
        .iter()
        .map(|(words, _)| program_word(words).unwrap_or_default())
        .collect();

    let mut out = Explained::default();
    let mut piped = false;
    for (index, (words, separator)) in segments.iter().enumerate() {
        if !words.is_empty() {
            command(words, piped, manuals, &mut out);
        }
        piped = false;
        let Some(op) = separator else { continue };
        let before = names[..=index].iter().rev().find(|n| !n.is_empty());
        let before = before.map_or("that", String::as_str);
        let after = names.get(index + 1).filter(|n| !n.is_empty());
        let after = after.map_or("the next command", String::as_str);
        let text = match op.value.as_str() {
            "|" => {
                piped = true;
                format!("sends what {before} prints into {after}")
            }
            "|&" => {
                piped = true;
                format!("sends what {before} prints, errors too, into {after}")
            }
            "&&" => format!("runs {after} only if {before} worked"),
            "||" => format!("runs {after} only if {before} failed"),
            ";" if index + 1 == segments.len() => "ends the command".to_string(),
            ";" => format!("then runs {after}, whether {before} worked or not"),
            "&" => format!("runs {before} in the background, so you get the prompt back"),
            "(" => "starts a subshell: what's inside runs in its own copy of the shell".into(),
            ")" => "ends the subshell".into(),
            _ => String::new(),
        };
        out.parts.push(Part {
            spans: vec![(op.start, op.end)],
            label: op.raw.clone(),
            text,
            more: Vec::new(),
            role: Role::Op,
            known: true,
        });
    }
    out
}

/// `:(){ :|:& };:` reads as noise, so it gets its own walkthrough.
fn fork_bomb(line: &str) -> Option<Explained> {
    if line.replace(' ', "") != ":(){:|:&};:" {
        return None;
    }
    let open = line.find('{')?;
    let close = line.rfind('}')?;
    let semi = line.rfind(';')?;
    let call = line.rfind(':')?;
    let piece = |spans: (usize, usize), label: &str, text: &str, role: Role| Part {
        spans: vec![spans],
        label: label.into(),
        text: text.into(),
        more: Vec::new(),
        role,
        known: true,
    };
    Some(Explained {
        parts: vec![
            piece(
                (0, line.find(')')? + 1),
                ":()",
                "makes a function called : (yes, just a colon)",
                Role::Program,
            ),
            piece(
                (open, close + 1),
                "{ :|:& }",
                "runs itself piped into itself, in the background: every call makes two more",
                Role::Flag,
            ),
            piece((semi, semi + 1), ";", "then", Role::Op),
            piece(
                (call, call + 1),
                ":",
                "calls it once. that's all it takes",
                Role::Program,
            ),
        ],
        commands: Vec::new(),
    })
}

fn is_separator(op: &str) -> bool {
    matches!(op, "|" | "|&" | "&&" | "||" | ";" | "&" | "(" | ")")
}

fn is_assignment(token: &Token) -> bool {
    let Some((name, _)) = token.value.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && !token.raw.starts_with(['"', '\''])
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

fn program_word(words: &[Token]) -> Option<String> {
    words
        .iter()
        .find(|t| t.kind == Kind::Word && !is_assignment(t))
        .map(|t| base_name(&t.value).to_string())
}

fn base_name(program: &str) -> &str {
    if program.starts_with("./") {
        return program;
    }
    program.rsplit('/').next().unwrap_or(program)
}

fn command(words: &[Token], piped: bool, manuals: &mut Manuals, out: &mut Explained) {
    let mut i = 0;
    let mut redirects = Vec::new();
    // `FOO=bar cmd`: variables set just for this command.
    while i < words.len() && words[i].kind == Kind::Word && is_assignment(&words[i]) {
        let (name, value) = words[i].value.split_once('=').unwrap_or_default();
        let only = words[i + 1..].iter().all(|t| t.kind == Kind::Op);
        let text = if only {
            format!("sets the shell variable {name} to {}", shown(value))
        } else {
            format!("sets {name} to {} just for this command", shown(value))
        };
        out.parts
            .push(part(&words[i], words[i].raw.clone(), text, Role::Arg, true));
        i += 1;
    }
    while i < words.len() && words[i].kind == Kind::Op {
        i = redirect(words, i, out, &mut redirects);
    }
    let Some(program_token) = words.get(i) else {
        return;
    };
    let program = program_token.value.clone();
    let name = base_name(&program).to_string();
    let mut page = if BUILTINS.iter().any(|(b, _)| *b == name) {
        None
    } else {
        manuals.get(&name, &[])
    };
    out.parts.push(Part {
        spans: vec![(program_token.start, program_token.end)],
        label: program_token.raw.clone(),
        text: program_text(&program, &name, page.as_deref()),
        more: Vec::new(),
        role: Role::Program,
        known: page.is_some() || BUILTINS.iter().any(|(b, _)| *b == name) || name == "uhh",
    });
    let mut cmd = Cmd {
        program: name.clone(),
        piped_in: piped,
        ..Cmd::default()
    };
    i += 1;

    let mut subs: Vec<String> = Vec::new();
    let mut pending: Option<usize> = None;
    let mut options_done = false;
    let mut positionals = 0;
    let mut skip_positionals = usize::from(name == "timeout");
    let wrapper = WRAPPERS.contains(&name.as_str());
    while i < words.len() {
        let token = &words[i];
        if token.kind == Kind::Op {
            pending = None;
            i = redirect(words, i, out, &mut redirects);
            continue;
        }
        if let Some(at) = pending.take() {
            let target = &mut out.parts[at];
            target.spans.push((token.start, token.end));
            target.label = format!("{} {}", target.label, token.raw);
            cmd.args.push(token.value.clone());
            i += 1;
            continue;
        }
        let value = token.value.as_str();
        let flag_like =
            value.len() > 1 && value.starts_with('-') && !token.raw.starts_with(['"', '\'']);
        if flag_like && !options_done {
            if value == "--" {
                options_done = true;
                out.parts.push(part(
                    token,
                    "--".into(),
                    "end of options: everything after this is a plain argument".into(),
                    Role::Flag,
                    true,
                ));
                i += 1;
                continue;
            }
            if name == "find" && matches!(value, "-exec" | "-execdir" | "-ok" | "-okdir") {
                i = find_exec(words, i, page.as_deref(), out);
                cmd.flags.push(value.to_string());
                continue;
            }
            pending = flag(token, &name, &subs, page.as_deref(), out, &mut cmd.flags);
            i += 1;
            continue;
        }

        // A plain word.
        if wrapper && skip_positionals == 0 && !(name == "env" && is_assignment(token)) {
            out.commands.push(std::mem::take(&mut cmd));
            command(&words[i..], piped, manuals, out);
            return;
        }
        if name == "env" && is_assignment(token) {
            let (var, val) = value.split_once('=').unwrap_or_default();
            let text = format!("sets {var} to {} for the command", shown(val));
            out.parts
                .push(part(token, token.raw.clone(), text, Role::Arg, true));
            i += 1;
            continue;
        }
        if positionals == 0
            && subs.is_empty()
            && DASHLESS.contains(&name.as_str())
            && let Some(found) = dashless(token, page.as_deref(), out, &mut cmd.flags)
        {
            pending = found;
            positionals += 1;
            i += 1;
            continue;
        }
        if positionals == subs.len() && subs.len() < 2 && !options_done && is_word(value) {
            let parent = page.clone();
            let mut path: Vec<&str> = subs.iter().map(String::as_str).collect();
            path.push(value);
            let listed = parent.as_ref().and_then(|p| p.word(value));
            // `git commit` has its own man page; `cargo build --help` is
            // only asked for when cargo's help lists `build`.
            let own = if listed.is_some() || !is_help_only(&name) {
                manuals.get(&name, &path)
            } else {
                None
            };
            if own.is_some() || listed.is_some() {
                let text = own
                    .as_ref()
                    .and_then(|p| p.summary.clone())
                    .filter(|s| !s.to_lowercase().starts_with("usage"))
                    .or(listed)
                    .unwrap_or_default();
                out.parts.push(part(
                    token,
                    token.raw.clone(),
                    clean_summary(&text),
                    Role::Sub,
                    true,
                ));
                if own.is_some() {
                    page = own;
                }
                subs.push(value.to_string());
                cmd.subcommand = Some(subs.join(" "));
                positionals += 1;
                i += 1;
                continue;
            }
        }
        let called = page
            .as_deref()
            .and_then(|p| p.argument(positionals - subs.len()))
            .map(str::to_string);
        let text = describe(token, &name, &subs, called.as_deref());
        out.parts
            .push(part(token, token.raw.clone(), text, Role::Arg, true));
        cmd.args.push(value.to_string());
        positionals += 1;
        skip_positionals = skip_positionals.saturating_sub(1);
        i += 1;
    }
    cmd.redirects = redirects;
    out.commands.push(cmd);
}

fn is_word(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn part(token: &Token, label: String, text: String, role: Role, known: bool) -> Part {
    Part {
        spans: vec![(token.start, token.end)],
        label,
        text,
        more: Vec::new(),
        role,
        known,
    }
}

fn program_text(program: &str, name: &str, page: Option<&Page>) -> String {
    match name {
        "uhh" => return "that's me (⊙_☉) I read the manual so you don't have to".into(),
        "welp" if page.is_none() => {
            return "fixes the command you just got wrong. we're friends".into();
        }
        _ => {}
    }
    if let Some((_, text)) = BUILTINS.iter().find(|(b, _)| *b == name) {
        return format!("{text} (a shell builtin)");
    }
    if let Some(summary) = page.and_then(|p| p.summary.as_deref()) {
        return clean_summary(summary);
    }
    if program.contains('/') {
        return if Path::new(program).exists() {
            format!("runs the program or script at {program}")
        } else {
            format!("runs {program}, but there's nothing there")
        };
    }
    if on_path(name) {
        format!("{name} is installed, but it has no manual here")
    } else {
        format!("{name} isn't installed here. a typo? `welp` fixes those")
    }
}

fn clean_summary(text: &str) -> String {
    let text = text.trim().trim_end_matches('.');
    // Start lowercase like the rest, unless it's an acronym (`GNU`) or a
    // name (`Rust's package manager`).
    let first_word = text.split_whitespace().next().unwrap_or("");
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second))
            if first.is_uppercase() && second.is_lowercase() && !first_word.contains('\'') =>
        {
            format!("{}{}", first.to_lowercase(), &text[first.len_utf8()..])
        }
        _ => text.to_string(),
    }
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// One option token: `-x`, `--file=a.tgz`, or a bundle like `-xzvf`.
/// Returns the part that takes the next word, if one does.
fn flag(
    token: &Token,
    program: &str,
    subs: &[String],
    page: Option<&Page>,
    out: &mut Explained,
    flags: &mut Vec<String>,
) -> Option<usize> {
    let value = token.value.as_str();
    let (name, inline) = match value.split_once('=') {
        Some((name, inline)) if name.starts_with("--") => (name, Some(inline)),
        _ => (value, None),
    };
    if matches!(program, "kill" | "killall" | "pkill")
        && let Some(text) = signal(&name[1..])
    {
        flags.push(name.to_string());
        out.parts
            .push(part(token, token.raw.clone(), text, Role::Flag, true));
        return None;
    }
    let who = if subs.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", subs.join(" "))
    };
    let found = page.and_then(|p| {
        p.option(name)
            .map(|e| (name.to_string(), e, false))
            .or_else(|| {
                p.option_prefix(name)
                    .map(|(n, e)| (n.to_string(), e, false))
            })
    });
    // `head -5`, `git log -3`: a number on its own means `-n <number>`.
    let found = found.or_else(|| {
        let digits = name.strip_prefix('-')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let entry = page?.option("-n")?;
        let arg = entry.arg.as_deref()?.to_lowercase();
        (arg.contains("num") || arg.contains("count") || arg.contains("lines"))
            .then(|| (name.to_string(), entry, true))
    });
    // `numeric`: the value is already in the word.
    if let Some((full, entry, numeric)) = found {
        flags.push(full.clone());
        let mut p = entry_part(token, entry, page);
        if full != name {
            p.text = format!("short for {full}. {}", p.text);
        }
        out.parts.push(p);
        return (entry.takes_arg() && inline.is_none() && !numeric).then(|| out.parts.len() - 1);
    }
    // `-xzvf`: several one-letter options at once.
    if !name.starts_with("--")
        && name.len() > 2
        && token.raw == token.value
        && let Some(page) = page
        && let Some(pending) = bundle(token, 1, page, out, flags)
    {
        return pending;
    }
    flags.push(name.to_string());
    let text = if page.is_some() {
        format!("not in {who}'s manual")
    } else {
        format!("an option for {who}")
    };
    out.parts
        .push(part(token, token.raw.clone(), text, Role::Flag, false));
    None
}

fn entry_part(token: &Token, entry: &Entry, page: Option<&Page>) -> Part {
    let mut text = entry.text.clone();
    let mut first = if text.is_empty() {
        String::new()
    } else {
        text.remove(0)
    };
    // `-r  Equivalent to -R.`: say what -R does too.
    if let Some(other) = alias_of(&first)
        && let Some(said) = page
            .and_then(|p| p.option(other))
            .and_then(|e| e.text.first())
    {
        first = format!("same as {other}: {said}");
    }
    Part {
        spans: vec![(token.start, token.end)],
        label: token.raw.clone(),
        text: first,
        more: text,
        role: Role::Flag,
        known: true,
    }
}

fn alias_of(text: &str) -> Option<&str> {
    let text = text.trim().trim_end_matches('.');
    let lower = text.to_lowercase();
    let prefix = [
        "equivalent to ",
        "same as ",
        "synonym for ",
        "an alias for ",
        "alias for ",
    ]
    .into_iter()
    .find(|p| lower.starts_with(p))?;
    let rest = &text[prefix.len()..];
    let rest = rest.trim_start_matches("the ").trim_end_matches(" option");
    (rest.starts_with('-') && !rest.contains(' ')).then_some(rest)
}

/// Splits `-xzvf` (from byte `from`) into `-x -z -v -f`, if every letter is
/// an option. `Some(Some(i))` when part `i` takes the next word.
fn bundle(
    token: &Token,
    from: usize,
    page: &Page,
    out: &mut Explained,
    flags: &mut Vec<String>,
) -> Option<Option<usize>> {
    let letters = &token.value[from..];
    let mut parts = Vec::new();
    let mut names = Vec::new();
    let mut pending = None;
    for (offset, letter) in letters.char_indices() {
        let name = format!("-{letter}");
        let entry = page.option(&name)?;
        let at = token.start + from + offset;
        // The first letter's colour covers the dash too.
        let start = if offset == 0 { token.start } else { at };
        let mut p = entry_part(token, entry, Some(page));
        p.spans = vec![(start, at + letter.len_utf8())];
        p.label = name.clone();
        names.push(name);
        if entry.takes_arg() {
            let rest = &letters[offset + letter.len_utf8()..];
            if !rest.is_empty() {
                // `-n5`: the rest of the word is the value.
                p.spans = vec![(start, token.end)];
                p.label = format!("{} {rest}", p.label);
                parts.push(p);
                break;
            }
            pending = Some(parts.len());
        }
        parts.push(p);
    }
    let base = out.parts.len();
    out.parts.extend(parts);
    flags.extend(names);
    Some(pending.map(|i| base + i))
}

/// `tar xzf a.tgz`, `ps aux`: options with no dash, first thing after.
fn dashless(
    token: &Token,
    page: Option<&Page>,
    out: &mut Explained,
    flags: &mut Vec<String>,
) -> Option<Option<usize>> {
    let page = page?;
    if token.value.starts_with('-') || token.raw != token.value || token.value.len() > 8 {
        return None;
    }
    if !token.value.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let found = bundle(token, 0, page, out, flags)?;
    Some(found)
}

/// `-exec rm {} \;`: everything up to the `;` or `+` is one command.
fn find_exec(words: &[Token], at: usize, page: Option<&Page>, out: &mut Explained) -> usize {
    let mut end = at + 1;
    while end < words.len() && !matches!(words[end].value.as_str(), ";" | "+") {
        end += 1;
    }
    let inner: Vec<&str> = words[at + 1..end.min(words.len())]
        .iter()
        .map(|t| t.raw.as_str())
        .collect();
    let last = end.min(words.len() - 1);
    let each = if words.get(end).is_some_and(|t| t.value == "+") {
        "once, with all the matches where {} is"
    } else {
        "on each match, with the match where {} is"
    };
    let mut p = Part {
        spans: vec![(words[at].start, words[last].end)],
        label: format!(
            "{} {}",
            words[at].raw,
            words[at + 1..=last]
                .iter()
                .map(|t| t.raw.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        text: format!("runs `{}` {each}", inner.join(" ")),
        more: Vec::new(),
        role: Role::Flag,
        known: true,
    };
    if let Some(entry) = page.and_then(|p| p.option(&words[at].value)) {
        p.more = entry.text.clone();
    }
    out.parts.push(p);
    end + 1
}

fn signal(name: &str) -> Option<String> {
    let name = name.trim_start_matches("s ").to_uppercase();
    let name = name.trim_start_matches("SIG");
    SIGNALS
        .iter()
        .find(|(number, short, _)| *number == name || *short == name)
        .map(|(number, short, what)| format!("sends signal {number} ({short}): {what}"))
}

fn redirect(
    words: &[Token],
    at: usize,
    out: &mut Explained,
    redirects: &mut Vec<(String, String)>,
) -> usize {
    let op = &words[at];
    let target = words.get(at + 1).filter(|t| t.kind == Kind::Word);
    let needs_target = !matches!(op.value.as_str(), "2>&1" | "1>&2");
    let t = target.map(|t| t.value.as_str()).unwrap_or("");
    let null = t == "/dev/null";
    let text = match op.value.as_str() {
        ">" | "1>" if null => "throws the output away".to_string(),
        ">" | "1>" => format!("writes the output to {t}, replacing what's in it"),
        ">>" => format!("adds the output to the end of {t}"),
        "2>" if null => "hides the errors".to_string(),
        "2>" => format!("writes the errors to {t}"),
        "2>>" => format!("adds the errors to the end of {t}"),
        "&>" | ">&" if null => "throws away the output and the errors".to_string(),
        ">&" if t == "2" => "sends the output to where errors go".to_string(),
        "&>" | ">&" => format!("writes the output and the errors to {t}"),
        "&>>" => format!("adds the output and the errors to the end of {t}"),
        "2>&1" => "sends errors to the same place as the output".to_string(),
        "1>&2" => "sends the output to where errors go".to_string(),
        "<" => format!("feeds {t} in as the input"),
        "<<<" => format!("feeds the text {} in as the input", shown(t)),
        "<<" => format!("feeds the lines up to {t} in as the input"),
        _ => String::new(),
    };
    let mut p = part(op, op.raw.clone(), text, Role::Op, true);
    let mut next = at + 1;
    if needs_target && let Some(target) = target {
        p.spans.push((target.start, target.end));
        p.label = format!("{} {}", op.raw, target.raw);
        redirects.push((op.value.clone(), target.value.clone()));
        next += 1;
    }
    out.parts.push(p);
    next
}

/// What a plain argument looks like it is.
fn describe(token: &Token, program: &str, subs: &[String], called: Option<&str>) -> String {
    let v = token.value.as_str();
    let who = if subs.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", subs.join(" "))
    };
    if let Some(inner) = v
        .strip_prefix("$(")
        .and_then(|s| s.strip_suffix(')'))
        .or_else(|| v.strip_prefix('`').and_then(|s| s.strip_suffix('`')))
    {
        return format!("the output of `{inner}`, run first");
    }
    if let Some(var) = whole_variable(v) {
        return match std::env::var(var) {
            Ok(value) if !value.is_empty() => {
                format!("the value of ${var}, which is {} here", shown(&value))
            }
            _ => format!("the value of ${var}, which isn't set here"),
        };
    }
    if !token.quoted && v.contains(['*', '?', '[']) {
        return "a pattern: your shell swaps it for every name that matches".into();
    }
    if v.contains("://") {
        return "a URL".into();
    }
    match v {
        "." => return "this folder".into(),
        ".." => return "the folder above this one".into(),
        "-" => return "standard input (or output), not a file".into(),
        "/" => return "the root of the whole disk".into(),
        "~" => return "your home folder".into(),
        "{}" if program == "xargs" => return "where each input line goes".into(),
        _ => {}
    }
    let expanded = match v.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(|h| Path::new(&h).join(rest)),
        None => Some(Path::new(v).to_path_buf()),
    };
    if let Some(path) = expanded {
        if path.is_dir() {
            return format!("the folder {v}");
        }
        if path.is_file() {
            return format!("the file {v}");
        }
    }
    if let Some(called) = called {
        return format!("the {called}");
    }
    if v.contains('/') {
        return "a path (nothing's there right now)".into();
    }
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
        return format!("the number {v}");
    }
    format!("an argument to {who}")
}

fn whole_variable(v: &str) -> Option<&str> {
    let name = v
        .strip_prefix("${")
        .and_then(|s| s.strip_suffix('}'))
        .or_else(|| v.strip_prefix('$'))?;
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then_some(name)
}

/// Quotes a value for a sentence, cut short if it's long.
fn shown(value: &str) -> String {
    let short: String = value.chars().take(40).collect();
    if short.len() < value.len() {
        format!("\"{short}…\"")
    } else {
        format!("\"{value}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAR: &str = "NAME
     tar - manipulate tape archives

     -x      Extract to disk from the archive.
     -z      Compress with gzip.
     -v      Produce verbose output.
     -f file, --file file
             Read the archive from or write the archive to the specified file.
";

    const GIT: &str = "NAME
       git - the stupid content tracker

       -C <path>
           Run as if git was started in <path>.
";

    const GIT_COMMIT: &str = "NAME
       git-commit - Record changes to the repository

       -a, --all
           Automatically stage files that have been modified and deleted.

       -m <msg>, --message=<msg>
           Use <msg> as the commit message.
";

    fn manuals() -> Manuals {
        let mut m = Manuals::default();
        m.insert("tar", Page::parse(TAR));
        m.insert("git", Page::parse(GIT));
        m.insert("git commit", Page::parse(GIT_COMMIT));
        m
    }

    fn labels(e: &Explained) -> Vec<&str> {
        e.parts.iter().map(|p| p.label.as_str()).collect()
    }

    #[test]
    fn bundles_split_and_take_their_value() {
        let line = "tar -xzvf backup.tgz";
        let e = explain(line, &mut manuals());
        assert_eq!(labels(&e), ["tar", "-x", "-z", "-v", "-f backup.tgz"]);
        let f = &e.parts[4];
        let colored: Vec<&str> = f.spans.iter().map(|&(a, b)| &line[a..b]).collect();
        assert_eq!(colored, ["f", "backup.tgz"]);
        assert_eq!(&line[e.parts[1].spans[0].0..e.parts[1].spans[0].1], "-x");
        assert_eq!(e.commands[0].flags, ["-x", "-z", "-v", "-f"]);
    }

    #[test]
    fn dashless_tar() {
        let e = explain("tar xzf a.tgz", &mut manuals());
        assert_eq!(labels(&e), ["tar", "-x", "-z", "-f a.tgz"]);
    }

    #[test]
    fn subcommands_use_their_own_page() {
        let e = explain("git -C repo commit -am \"wip\"", &mut manuals());
        assert_eq!(labels(&e), ["git", "-C repo", "commit", "-a", "-m \"wip\""]);
        assert_eq!(e.parts[2].text, "record changes to the repository");
        assert_eq!(e.parts[3].role, Role::Flag);
        assert_eq!(e.commands[0].subcommand.as_deref(), Some("commit"));
    }

    #[test]
    fn long_options_with_values() {
        let e = explain("git commit --message=hi --all", &mut manuals());
        assert_eq!(labels(&e), ["git", "commit", "--message=hi", "--all"]);
        assert!(e.parts[2].known && e.parts[2].text.starts_with("Use <msg>"));
    }

    #[test]
    fn pipes_and_redirects() {
        let e = explain("tar -tf a.tgz | grep x > out.txt 2>&1", &mut manuals());
        let pipe = e.parts.iter().find(|p| p.label == "|").unwrap();
        assert_eq!(pipe.text, "sends what tar prints into grep");
        let to = e.parts.iter().find(|p| p.label == "> out.txt").unwrap();
        assert_eq!(
            to.text,
            "writes the output to out.txt, replacing what's in it"
        );
        assert!(e.parts.iter().any(|p| p.label == "2>&1"));
        assert!(e.commands[1].piped_in);
        assert_eq!(e.commands[1].redirects[0].1, "out.txt");
    }

    #[test]
    fn wrappers_explain_what_they_run() {
        let e = explain("sudo -u bob tar -x", &mut manuals());
        let programs: Vec<&str> = e
            .parts
            .iter()
            .filter(|p| p.role == Role::Program)
            .map(|p| p.label.as_str())
            .collect();
        assert_eq!(programs, ["sudo", "tar"]);
        assert_eq!(e.commands.len(), 2);
    }

    #[test]
    fn arguments_say_what_they_are() {
        let e = explain("frob $(date) https://x.dev . '*.rs' *.rs", &mut manuals());
        let texts: Vec<&str> = e.parts[1..].iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts[0], "the output of `date`, run first");
        assert_eq!(texts[1], "a URL");
        assert_eq!(texts[2], "this folder");
        assert_eq!(texts[3], "an argument to frob");
        assert!(texts[4].starts_with("a pattern"));
    }

    #[test]
    fn assignments_and_builtins() {
        let e = explain("FOO=1 cd /tmp", &mut manuals());
        assert_eq!(e.parts[0].text, "sets FOO to \"1\" just for this command");
        assert!(e.parts[1].text.ends_with("(a shell builtin)"));
    }

    #[test]
    fn aliases_say_what_they_stand_for() {
        let mut m = Manuals::default();
        m.insert(
            "rm",
            Page::parse(
                "     -R      Remove the file hierarchy.\n     -r      Equivalent to -R.\n",
            ),
        );
        let e = explain("rm -r x", &mut m);
        assert_eq!(e.parts[1].text, "same as -R: Remove the file hierarchy.");
    }

    #[test]
    fn a_bare_number_is_dash_n() {
        let mut m = Manuals::default();
        m.insert(
            "head",
            Page::parse(
                "     -n count, --lines=count\n             Print count lines of each file.\n",
            ),
        );
        let e = explain("head -5 notes.txt", &mut m);
        assert_eq!(e.parts[1].label, "-5");
        assert_eq!(e.parts[1].text, "Print count lines of each file.");
        assert_eq!(e.parts[2].label, "notes.txt");
    }

    #[test]
    fn fork_bombs_get_a_walkthrough() {
        let line = ":(){ :|:& };:";
        let e = explain(line, &mut Manuals::default());
        let labels: Vec<&str> = e.parts.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, [":()", "{ :|:& }", ";", ":"]);
        let (a, b) = e.parts[1].spans[0];
        assert_eq!(&line[a..b], "{ :|:& }");
    }

    #[test]
    fn kill_signals() {
        let e = explain("kill -9 123", &mut Manuals::default());
        assert!(e.parts[1].text.starts_with("sends signal 9 (KILL)"));
    }

    #[test]
    fn find_exec_is_one_part() {
        let e = explain(
            r"find . -name '*.log' -exec rm {} \;",
            &mut Manuals::default(),
        );
        let exec = e
            .parts
            .iter()
            .find(|p| p.label.starts_with("-exec"))
            .unwrap();
        assert_eq!(exec.label, r"-exec rm {} \;");
        assert!(exec.text.starts_with("runs `rm {}` on each match"));
    }

    #[test]
    fn unknown_options_say_so() {
        let e = explain("tar -Q", &mut manuals());
        assert_eq!(e.parts[1].text, "not in tar's manual");
        assert!(!e.parts[1].known);
    }
}
