//! How uhh looks: the command with each part in its own colour, then one
//! line per part in the same colour, then anything to watch out for.

use crossterm::style::{Color, Stylize};

use crate::explain::{Explained, Part, Role};

const COLORS: [Color; 6] = [
    Color::Cyan,
    Color::Magenta,
    Color::Yellow,
    Color::Green,
    Color::Blue,
    Color::DarkCyan,
];

/// Lines of text per part before it's cut short (without `--full`).
const LINES: usize = 2;

#[derive(Clone, Copy)]
pub struct Paint {
    pub color: bool,
}

impl Paint {
    pub fn badge(self) -> String {
        if self.color {
            " uhh "
                .with(Color::Black)
                .on(Color::Rgb {
                    r: 125,
                    g: 211,
                    b: 252,
                })
                .bold()
                .to_string()
        } else {
            "uhh:".to_string()
        }
    }

    fn paint(self, text: &str, color: Option<Color>) -> String {
        match color {
            Some(color) if self.color => text.with(color).bold().to_string(),
            _ => text.to_string(),
        }
    }

    pub fn dim(self, text: &str) -> String {
        if self.color {
            text.dark_grey().to_string()
        } else {
            text.to_string()
        }
    }

    fn warn(self, text: &str) -> String {
        if self.color {
            text.with(Color::Rgb {
                r: 255,
                g: 135,
                b: 95,
            })
            .bold()
            .to_string()
        } else {
            text.to_string()
        }
    }

    /// Something uhh says on its own, after the badge.
    pub fn line(self, text: &str) -> String {
        format!("{}  {text}", self.badge())
    }
}

pub fn render(
    line: &str,
    explained: &Explained,
    warnings: &[String],
    paint: Paint,
    width: usize,
    full: bool,
) -> String {
    let colors = colors(&explained.parts);
    let mut out = String::new();
    out.push_str(&paint.line(&command_line(line, explained, &colors, paint)));
    out.push_str("\n\n");

    let indent = |part: &Part| match part.role {
        Role::Program | Role::Op => 2,
        _ => 4,
    };
    let labels: Vec<String> = explained
        .parts
        .iter()
        .map(|p| short(&p.label, 32))
        .collect();
    let column = explained
        .parts
        .iter()
        .zip(&labels)
        .map(|(p, label)| indent(p) + label.chars().count())
        .filter(|&w| w <= 26)
        .max()
        .unwrap_or(8)
        + 2;
    let room = width.saturating_sub(column).max(24);

    for ((part, label), color) in explained.parts.iter().zip(&labels).zip(&colors) {
        let pad = indent(part);
        let used = pad + label.chars().count();
        out.push_str(&" ".repeat(pad));
        out.push_str(&paint.paint(label, *color));
        let text = if full {
            part.text.clone()
        } else {
            shorten(&part.text, room * LINES)
        };
        let mut lines = wrap(&text, room);
        if full {
            for more in &part.more {
                lines.push(String::new());
                lines.extend(wrap(more, room));
            }
        }
        let mut first = true;
        for text in lines {
            if first && used + 2 <= column {
                out.push_str(&" ".repeat(column - used));
            } else {
                out.push('\n');
                out.push_str(&" ".repeat(column));
            }
            first = false;
            if part.known {
                out.push_str(&text);
            } else {
                out.push_str(&paint.dim(&text));
            }
        }
        out.push('\n');
    }

    if !warnings.is_empty() {
        out.push('\n');
        let tag = "heads up";
        let room = width.saturating_sub(2 + tag.len() + 2).max(24);
        for warning in warnings {
            for (i, text) in wrap(warning, room).iter().enumerate() {
                if i == 0 {
                    out.push_str(&format!("  {}  {text}\n", paint.warn(tag)));
                } else {
                    out.push_str(&format!("{}{text}\n", " ".repeat(2 + tag.len() + 2)));
                }
            }
        }
    }
    out
}

/// A colour for each part: operators stay plain, the rest take turns.
fn colors(parts: &[Part]) -> Vec<Option<Color>> {
    let mut next = 0;
    parts
        .iter()
        .map(|part| {
            if part.role == Role::Op {
                None
            } else {
                next += 1;
                Some(COLORS[(next - 1) % COLORS.len()])
            }
        })
        .collect()
}

/// The command as typed, each part in its colour.
fn command_line(
    line: &str,
    explained: &Explained,
    colors: &[Option<Color>],
    paint: Paint,
) -> String {
    let line = line.trim_end();
    let mut owner: Vec<Option<usize>> = vec![None; line.len()];
    for (index, part) in explained.parts.iter().enumerate() {
        for &(start, end) in &part.spans {
            for slot in owner.iter_mut().take(end.min(line.len())).skip(start) {
                *slot = Some(index);
            }
        }
    }
    let mut out = String::new();
    let mut run = String::new();
    let mut run_owner = None;
    for (offset, c) in line.char_indices() {
        let who = owner[offset];
        if who != run_owner && !run.is_empty() {
            out.push_str(&paint.paint(&run, run_owner.and_then(|i| colors[i])));
            run.clear();
        }
        run_owner = who;
        run.push(c);
    }
    out.push_str(&paint.paint(&run, run_owner.and_then(|i| colors[i])));
    out
}

/// The first sentence (and the next, if the first is short and both fit
/// in `max` characters), or as many words as fit.
fn shorten(text: &str, max: usize) -> String {
    let mut ends = Vec::new();
    for (at, _) in text.match_indices(". ") {
        let word = text[..at].rsplit(' ').next().unwrap_or("");
        let word = word.trim_start_matches('(').to_lowercase();
        if !matches!(word.as_str(), "e.g" | "i.e" | "etc" | "vs" | "cf" | "no") && word.len() > 1 {
            ends.push(at + 1);
        }
    }
    ends.push(text.len());
    let mut keep = 0;
    for end in ends {
        if text[..end].chars().count() > max {
            break;
        }
        keep = end;
        if keep >= 30 {
            break;
        }
    }
    if keep > 0 {
        return text[..keep].to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let at = cut.rfind(' ').unwrap_or(cut.len());
    format!("{}…", cut[..at].trim_end_matches([',', ';', ':']))
}

fn short(label: &str, max: usize) -> String {
    if label.chars().count() <= max {
        label.to_string()
    } else {
        format!("{}…", label.chars().take(max - 1).collect::<String>())
    }
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    // Only plain spaces break a line, so `|\u{a0}bash` stays together.
    for word in text.split([' ', '\t', '\n']).filter(|w| !w.is_empty()) {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explain::{Manuals, explain};

    #[test]
    fn plain_output_lines_up() {
        let line = "frob hi > /dev/null";
        let e = explain(line, &mut Manuals::default());
        let out = render(line, &e, &[], Paint { color: false }, 80, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "uhh:  frob hi > /dev/null");
        assert!(lines[3].starts_with("    hi") && lines[3].ends_with("an argument to frob"));
        assert!(
            lines[4].starts_with("  > /dev/null") && lines[4].ends_with("throws the output away")
        );
        let column = lines[3].find("an argument").unwrap();
        assert_eq!(lines[4].find("throws").unwrap(), column);
    }

    #[test]
    fn colour_keeps_the_command_readable() {
        let line = "echo hi | cat";
        let e = explain(line, &mut Manuals::default());
        let out = render(
            line,
            &e,
            &["careful".into()],
            Paint { color: true },
            80,
            false,
        );
        let first = out.lines().next().unwrap();
        let stripped: String = strip(first);
        assert!(stripped.ends_with("echo hi | cat"));
        assert!(strip(&out).contains("heads up  careful"));
    }

    #[test]
    fn long_text_is_cut_at_a_sentence() {
        let text = "Extract to disk from the archive. If a file with the same name appears more than once, later copies win.";
        assert_eq!(shorten(text, 200), "Extract to disk from the archive.");
        let short = "Produce output. Twice as much (i.e. more) if given twice. Then more.";
        assert_eq!(
            shorten(short, 200),
            "Produce output. Twice as much (i.e. more) if given twice."
        );
        assert_eq!(shorten("one two three four", 12), "one two…");
        assert_eq!(wrap("aa bb cc", 5), ["aa bb", "cc"]);
    }

    fn strip(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }
}
