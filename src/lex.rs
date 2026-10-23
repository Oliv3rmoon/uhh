//! Splits a command line into words and operators the way a POSIX shell
//! would, keeping where each one sits in the line so it can be coloured in
//! place.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Word,
    /// `|`, `&&`, `;`, `>`, `2>&1`, ...
    Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// The text as typed, quotes and all.
    pub raw: String,
    /// The text the program receives, with quotes and escapes removed.
    pub value: String,
    /// Byte offsets of `raw` in the line.
    pub start: usize,
    pub end: usize,
    /// Typed with quotes or escapes somewhere in it, so `*` stays a `*`.
    pub quoted: bool,
}

/// Longest first, so `>>` wins over `>`.
const OPS: [&str; 19] = [
    "&>>", "<<<", "2>&1", "1>&2", "2>>", "&>", ">&", ">>", "<<", "||", "&&", "|&", "2>", "1>", "|",
    ";", "&", ">", "<",
];

pub fn lex(line: &str) -> Vec<Token> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'#' {
            break;
        }
        if let Some(op) = OPS.iter().find(|op| line[i..].starts_with(*op)) {
            // `2>` is only an operator at the start of a word: `a2>b` isn't.
            tokens.push(Token {
                kind: Kind::Op,
                raw: op.to_string(),
                value: op.to_string(),
                start: i,
                end: i + op.len(),
                quoted: false,
            });
            i += op.len();
            continue;
        }
        if c == b'(' || c == b')' {
            let text = (c as char).to_string();
            tokens.push(Token {
                kind: Kind::Op,
                raw: text.clone(),
                value: text,
                start: i,
                end: i + 1,
                quoted: false,
            });
            i += 1;
            continue;
        }
        let (token, next) = word(line, i);
        tokens.push(token);
        i = next;
    }
    tokens
}

fn word(line: &str, start: usize) -> (Token, usize) {
    let chars: Vec<(usize, char)> = line[start..].char_indices().collect();
    let mut value = String::new();
    let mut quoted = false;
    let mut k = 0;
    while k < chars.len() {
        let c = chars[k].1;
        match c {
            c if c.is_whitespace() => break,
            '|' | '&' | ';' | '<' | '>' | '(' | ')' => break,
            '\\' => {
                quoted = true;
                if let Some(&(_, next)) = chars.get(k + 1) {
                    value.push(next);
                    k += 2;
                } else {
                    k += 1;
                }
            }
            '\'' => {
                quoted = true;
                k += 1;
                while k < chars.len() && chars[k].1 != '\'' {
                    value.push(chars[k].1);
                    k += 1;
                }
                k += 1;
            }
            '"' => {
                quoted = true;
                k += 1;
                while k < chars.len() && chars[k].1 != '"' {
                    if chars[k].1 == '\\'
                        && matches!(chars.get(k + 1), Some((_, '"' | '\\' | '$' | '`')))
                    {
                        k += 1;
                    }
                    if chars[k].1 == '$' && matches!(chars.get(k + 1), Some((_, '('))) {
                        k = substitution(&chars, k, &mut value);
                        continue;
                    }
                    value.push(chars[k].1);
                    k += 1;
                }
                k += 1;
            }
            '$' if matches!(chars.get(k + 1), Some((_, '(' | '{'))) => {
                k = substitution(&chars, k, &mut value);
            }
            '`' => {
                value.push('`');
                k += 1;
                while k < chars.len() && chars[k].1 != '`' {
                    value.push(chars[k].1);
                    k += 1;
                }
                if k < chars.len() {
                    value.push('`');
                }
                k += 1;
            }
            _ => {
                value.push(c);
                k += 1;
            }
        }
    }
    let end = chars
        .get(k)
        .map_or(line.len(), |&(offset, _)| start + offset);
    let end = end.min(line.len());
    (
        Token {
            kind: Kind::Word,
            raw: line[start..end].to_string(),
            value,
            start,
            end,
            quoted,
        },
        end,
    )
}

/// Copies `$(...)` or `${...}` through to its closing bracket, nesting and
/// quotes included, and returns the index after it.
fn substitution(chars: &[(usize, char)], mut k: usize, value: &mut String) -> usize {
    let (open, close) = if chars[k + 1].1 == '(' {
        ('(', ')')
    } else {
        ('{', '}')
    };
    let mut depth = 0;
    let mut quote: Option<char> = None;
    while k < chars.len() {
        let c = chars[k].1;
        value.push(c);
        k += 1;
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, c) if c == open => depth += 1,
            (None, c) if c == close => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(line: &str) -> Vec<String> {
        lex(line).into_iter().map(|t| t.value).collect()
    }

    #[test]
    fn words_and_operators() {
        assert_eq!(
            values("curl -fsSL https://x.sh | sh && echo done; ls>out 2>&1"),
            [
                "curl",
                "-fsSL",
                "https://x.sh",
                "|",
                "sh",
                "&&",
                "echo",
                "done",
                ";",
                "ls",
                ">",
                "out",
                "2>&1"
            ]
        );
    }

    #[test]
    fn quotes_and_escapes() {
        let tokens = lex(r#"git commit -m "fix: it's \"done\"" 'a b' c\ d"#);
        let v: Vec<&str> = tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(
            v,
            ["git", "commit", "-m", r#"fix: it's "done""#, "a b", "c d"]
        );
        assert_eq!(tokens[3].raw, r#""fix: it's \"done\"""#);
        assert!(tokens[3].quoted && !tokens[2].quoted);
    }

    #[test]
    fn substitutions_stay_whole() {
        assert_eq!(
            values("echo $(date +%F | tr - _) \"$(whoami)\" ${HOME}/x `id -u`"),
            [
                "echo",
                "$(date +%F | tr - _)",
                "$(whoami)",
                "${HOME}/x",
                "`id -u`"
            ]
        );
    }

    #[test]
    fn offsets_point_into_the_line() {
        let line = "tar  -xzvf   a.tgz";
        for token in lex(line) {
            assert_eq!(&line[token.start..token.end], token.raw);
        }
    }

    #[test]
    fn comments_end_the_line() {
        assert_eq!(values("ls -la # list"), ["ls", "-la"]);
    }

    #[test]
    fn find_semicolon() {
        assert_eq!(
            values(r"find . -exec rm {} \;"),
            ["find", ".", "-exec", "rm", "{}", ";"]
        );
    }
}
