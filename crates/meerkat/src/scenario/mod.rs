/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The shared scenario vocabulary: one ordered list of registry-id steps that both
//! the headless agent path (execute + assert against state) and the headed
//! self-drive (execute + capture frames) consume.
//!
//! A scenario is the automation unification named in the headed-automation plan.
//! The in-process harness and the on-screen app verify different layers (state vs
//! pixels), but they can share one *description*: registry command ids plus a few
//! host verbs (window spawn) and harness markers (capture / settle / assert / log).
//! A regression is written once here and gives both a headless assertion and a
//! headed screenshot.
//!
//! This module is the pure half: the [`Step`] vocabulary and its [`parse`]r, no
//! `Shell` dependency, so it stays trivially testable. The runtime half (the headed
//! self-drive pumped on the event loop, and the `Shell`-side executors both runners
//! share) is [`runner`].

mod runner;
pub(crate) use runner::ScenarioRunner;

/// One scenario step. The vocabulary is the command-registry id space (`invoke`)
/// plus the host verbs and harness markers a driven session needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Invoke a registry command or context-action by its stable id (`settings`,
    /// `roster`, `relate`, ...) on window `win` (0 = primary). The one by-id seam
    /// the palette, menu, agent harness, and a11y already share.
    Invoke { id: String, win: usize },
    /// Navigate window `win` to `url` through the omnibar-submit path (classify +
    /// resolve + history + load), the same route Enter in the address bar takes. For
    /// flows that are *not* registry commands. Async: follow with `settle` for the load.
    Navigate { url: String, win: usize },
    /// Dispatch a key chord (`ctrl+f`, `enter`, `f5`, `escape`, a bare char) to window
    /// `win` through the real `on_key_pressed` path, reading the same modifier state OS
    /// input sets. For the chords that are not registry commands (find, omnibar typing).
    Key { chord: KeyChord, win: usize },
    /// Set the active theme by id on window `win`.
    Theme { id: String, win: usize },
    /// Spawn a new window over the shared session (the Ctrl+Shift+N chord, as a verb).
    Spawn,
    /// Self-capture window `win`'s chrome texture to `<capture_dir>/<name>.png` (the
    /// GPU self-capture hook, compositor-independent). Needs `MEERKAT_CAPTURE_DIR`.
    Capture { name: String, win: usize },
    /// Yield `frames` event-loop ticks so queued work applies and renders before the
    /// next step (a spawned window's first frame, an async settle).
    Settle { frames: u32 },
    /// A checked assertion. Headless fails the test on a miss; headed logs the outcome.
    Assert(Assertion),
    /// Emit a marker line into the scenario log and the diagnostics ring.
    Log { message: String },
}

/// A scenario assertion. Deliberately tiny for now (the live window count is the
/// multi-window motivator); new predicates join as scenarios need them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Assertion {
    /// The live window count compares `op` against `n`.
    Windows { op: Cmp, n: usize },
}

/// A parsed key chord: the modifier flags plus the key. Winit-free so the parser stays
/// pure; the runner maps [`KeySym`] onto `winit::keyboard::Key` at dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyChord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub key: KeySym,
}

/// The key half of a chord: a literal character, or a named key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeySym {
    Char(char),
    Named(NamedKey),
}

/// The named (non-character) keys a scenario can dispatch. `F(n)` is a function key,
/// validated `1..=12` at parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NamedKey {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Space,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

/// A comparison operator, shared by every numeric assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cmp {
    Eq,
    Ne,
    Ge,
    Le,
    Gt,
    Lt,
}

impl Cmp {
    fn parse(s: &str) -> Option<Cmp> {
        Some(match s {
            "==" => Cmp::Eq,
            "!=" => Cmp::Ne,
            ">=" => Cmp::Ge,
            "<=" => Cmp::Le,
            ">" => Cmp::Gt,
            "<" => Cmp::Lt,
            _ => return None,
        })
    }

    pub(crate) fn eval(self, a: usize, b: usize) -> bool {
        match self {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            Cmp::Ge => a >= b,
            Cmp::Le => a <= b,
            Cmp::Gt => a > b,
            Cmp::Lt => a < b,
        }
    }

    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Cmp::Eq => "==",
            Cmp::Ne => "!=",
            Cmp::Ge => ">=",
            Cmp::Le => "<=",
            Cmp::Gt => ">",
            Cmp::Lt => "<",
        }
    }
}

/// A parse failure with the 1-based source line for the driver's error report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScenarioError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Parse a scenario source into its ordered [`Step`]s. Blank lines and `#` comments
/// are skipped. A trailing `@<n>` token on a windowed verb selects the target window
/// (default 0, the primary). The grammar is line-oriented and whitespace-tokenized:
///
/// ```text
/// # a comment
/// invoke  settings          # open the settings lane on the primary
/// spawn                     # open a second window
/// settle  2                 # let it apply + render
/// invoke  roster @1         # act in window 1
/// assert  windows == 2
/// capture both @1           # self-capture window 1's chrome
/// log     done
/// ```
pub(crate) fn parse(src: &str) -> Result<Vec<Step>, ScenarioError> {
    let mut steps = Vec::new();
    for (i, raw) in src.lines().enumerate() {
        let line = i + 1;
        let content = strip_comment(raw).trim();
        if content.is_empty() {
            continue;
        }
        steps.push(parse_line(content, line)?);
    }
    Ok(steps)
}

/// Strip a trailing `#` comment, but only when the `#` starts the line or follows
/// whitespace — so a `#fragment` inside a `navigate` URL is not mistaken for a comment.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            return &line[..i];
        }
    }
    line
}

fn parse_line(content: &str, line: usize) -> Result<Step, ScenarioError> {
    let err = |message: String| ScenarioError { line, message };
    let mut tokens: Vec<&str> = content.split_whitespace().collect();
    // A trailing `@<n>` sets the target window for the windowed verbs. Pop it here so
    // each verb parses its own operands without minding it.
    let win = match tokens.last().and_then(|t| t.strip_prefix('@')) {
        Some(n) => {
            let parsed = n
                .parse::<usize>()
                .map_err(|_| err(format!("bad window selector `@{n}`")))?;
            tokens.pop();
            parsed
        }
        None => 0,
    };
    let verb = tokens[0];
    let rest = &tokens[1..];
    match verb {
        "invoke" => {
            let id = single_operand(rest, "invoke", line)?;
            Ok(Step::Invoke { id, win })
        }
        "navigate" => {
            let url = single_operand(rest, "navigate", line)?;
            Ok(Step::Navigate { url, win })
        }
        "key" => {
            let spec = single_operand(rest, "key", line)?;
            let chord = parse_chord(&spec, line)?;
            Ok(Step::Key { chord, win })
        }
        "theme" => {
            let id = single_operand(rest, "theme", line)?;
            Ok(Step::Theme { id, win })
        }
        "spawn" => {
            require_empty(rest, "spawn", line)?;
            Ok(Step::Spawn)
        }
        "capture" => {
            let name = single_operand(rest, "capture", line)?;
            Ok(Step::Capture { name, win })
        }
        "settle" => {
            let frames = match rest.first() {
                None => 1,
                Some(n) => n
                    .parse::<u32>()
                    .map_err(|_| err(format!("settle expects a frame count, got `{n}`")))?,
            };
            Ok(Step::Settle { frames })
        }
        "assert" => parse_assert(rest, line),
        "log" => Ok(Step::Log {
            message: rest.join(" "),
        }),
        other => Err(err(format!("unknown verb `{other}`"))),
    }
}

fn parse_assert(rest: &[&str], line: usize) -> Result<Step, ScenarioError> {
    let err = |message: String| ScenarioError { line, message };
    match rest {
        ["windows", op, n] => {
            let op = Cmp::parse(op).ok_or_else(|| err(format!("bad comparison `{op}`")))?;
            let n = n
                .parse::<usize>()
                .map_err(|_| err(format!("assert windows expects a count, got `{n}`")))?;
            Ok(Step::Assert(Assertion::Windows { op, n }))
        }
        ["windows", ..] => Err(err(
            "assert windows expects `<op> <count>` (e.g. `windows == 2`)".to_string(),
        )),
        [target, ..] => Err(err(format!("unknown assert target `{target}`"))),
        [] => Err(err("assert expects a target".to_string())),
    }
}

/// Parse a chord like `ctrl+shift+n`, `enter`, `f5`, or a bare `f`: the last `+`-part is
/// the key, the earlier ones are modifiers.
fn parse_chord(spec: &str, line: usize) -> Result<KeyChord, ScenarioError> {
    let err = |message: String| ScenarioError { line, message };
    let parts: Vec<&str> = spec.split('+').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(err(format!("malformed chord `{spec}`")));
    }
    let (mods, last) = parts.split_at(parts.len() - 1);
    let (mut ctrl, mut shift, mut alt, mut meta) = (false, false, false, false);
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "shift" => shift = true,
            "alt" | "option" => alt = true,
            "meta" | "cmd" | "super" | "win" => meta = true,
            other => return Err(err(format!("unknown modifier `{other}` in chord `{spec}`"))),
        }
    }
    let key = parse_key_sym(last[0])
        .ok_or_else(|| err(format!("unknown key `{}` in chord `{spec}`", last[0])))?;
    Ok(KeyChord {
        ctrl,
        shift,
        alt,
        meta,
        key,
    })
}

/// Resolve one key token to a [`KeySym`]: a named key, an `f1`..`f12`, or a single char.
fn parse_key_sym(tok: &str) -> Option<KeySym> {
    let named = match tok.to_ascii_lowercase().as_str() {
        "enter" | "return" => NamedKey::Enter,
        "escape" | "esc" => NamedKey::Escape,
        "tab" => NamedKey::Tab,
        "backspace" => NamedKey::Backspace,
        "delete" | "del" => NamedKey::Delete,
        "space" => NamedKey::Space,
        "up" => NamedKey::Up,
        "down" => NamedKey::Down,
        "left" => NamedKey::Left,
        "right" => NamedKey::Right,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "pageup" | "pgup" => NamedKey::PageUp,
        "pagedown" | "pgdn" => NamedKey::PageDown,
        other => {
            // f1..f12, else a single character (case preserved for typing into a field).
            if let Some(n) = other.strip_prefix('f').and_then(|d| d.parse::<u8>().ok()) {
                return (1..=12).contains(&n).then_some(KeySym::Named(NamedKey::F(n)));
            }
            let mut chars = tok.chars();
            let c = chars.next()?;
            return chars.next().is_none().then_some(KeySym::Char(c));
        }
    };
    Some(KeySym::Named(named))
}

fn single_operand(rest: &[&str], verb: &str, line: usize) -> Result<String, ScenarioError> {
    match rest {
        [one] => Ok((*one).to_string()),
        [] => Err(ScenarioError {
            line,
            message: format!("{verb} expects an operand"),
        }),
        _ => Err(ScenarioError {
            line,
            message: format!("{verb} expects exactly one operand"),
        }),
    }
}

fn require_empty(rest: &[&str], verb: &str, line: usize) -> Result<(), ScenarioError> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(ScenarioError {
            line,
            message: format!("{verb} takes no operands"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_full_grammar() {
        let src = "\
# multi-window smoke
invoke settings
spawn
settle 2
invoke roster @1
theme dark @1
assert windows == 2
capture both @1
log all done
";
        let steps = parse(src).expect("parses");
        assert_eq!(
            steps,
            vec![
                Step::Invoke {
                    id: "settings".into(),
                    win: 0
                },
                Step::Spawn,
                Step::Settle { frames: 2 },
                Step::Invoke {
                    id: "roster".into(),
                    win: 1
                },
                Step::Theme {
                    id: "dark".into(),
                    win: 1
                },
                Step::Assert(Assertion::Windows { op: Cmp::Eq, n: 2 }),
                Step::Capture {
                    name: "both".into(),
                    win: 1
                },
                Step::Log {
                    message: "all done".into()
                },
            ]
        );
    }

    #[test]
    fn skips_blanks_and_comments_and_trailing_comments() {
        let steps = parse("\n  \n# just a comment\ninvoke home  # trailing\n").expect("parses");
        assert_eq!(
            steps,
            vec![Step::Invoke {
                id: "home".into(),
                win: 0
            }]
        );
    }

    #[test]
    fn settle_defaults_to_one_frame() {
        assert_eq!(
            parse("settle").unwrap(),
            vec![Step::Settle { frames: 1 }]
        );
    }

    #[test]
    fn reports_the_line_on_an_unknown_verb() {
        let err = parse("invoke home\nwiggle\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("wiggle"), "{}", err.message);
    }

    #[test]
    fn rejects_a_bad_window_selector() {
        let err = parse("invoke settings @x").unwrap_err();
        assert!(err.message.contains("window selector"), "{}", err.message);
    }

    #[test]
    fn rejects_a_malformed_assert() {
        assert!(parse("assert windows =? 2").is_err());
        assert!(parse("assert windows ==").is_err());
        assert!(parse("assert nodes == 2").is_err());
    }

    #[test]
    fn parses_navigate_and_preserves_url_fragments() {
        // The `#frag` must survive: only a whitespace-preceded `#` is a comment.
        let steps = parse("navigate https://example.com/a#frag  # go there").expect("parses");
        assert_eq!(
            steps,
            vec![Step::Navigate {
                url: "https://example.com/a#frag".into(),
                win: 0
            }]
        );
    }

    #[test]
    fn parses_key_chords() {
        let steps = parse("key ctrl+f\nkey ctrl+shift+n @1\nkey enter\nkey f5\nkey a").unwrap();
        assert_eq!(
            steps,
            vec![
                Step::Key {
                    chord: KeyChord {
                        ctrl: true,
                        shift: false,
                        alt: false,
                        meta: false,
                        key: KeySym::Char('f')
                    },
                    win: 0
                },
                Step::Key {
                    chord: KeyChord {
                        ctrl: true,
                        shift: true,
                        alt: false,
                        meta: false,
                        key: KeySym::Char('n')
                    },
                    win: 1
                },
                Step::Key {
                    chord: KeyChord {
                        ctrl: false,
                        shift: false,
                        alt: false,
                        meta: false,
                        key: KeySym::Named(NamedKey::Enter)
                    },
                    win: 0
                },
                Step::Key {
                    chord: KeyChord {
                        ctrl: false,
                        shift: false,
                        alt: false,
                        meta: false,
                        key: KeySym::Named(NamedKey::F(5))
                    },
                    win: 0
                },
                Step::Key {
                    chord: KeyChord {
                        ctrl: false,
                        shift: false,
                        alt: false,
                        meta: false,
                        key: KeySym::Char('a')
                    },
                    win: 0
                },
            ]
        );
    }

    #[test]
    fn rejects_bad_chords() {
        assert!(parse("key ctrl+").is_err()); // empty key part
        assert!(parse("key hyper+f").is_err()); // unknown modifier
        assert!(parse("key f13").is_err()); // out-of-range function key
        assert!(parse("key wiggle").is_err()); // multi-char, not a known name
    }

    #[test]
    fn cmp_evaluates_each_operator() {
        assert!(Cmp::Eq.eval(2, 2));
        assert!(Cmp::Ne.eval(1, 2));
        assert!(Cmp::Ge.eval(2, 2) && Cmp::Ge.eval(3, 2));
        assert!(Cmp::Le.eval(2, 2) && Cmp::Le.eval(1, 2));
        assert!(Cmp::Gt.eval(3, 2) && !Cmp::Gt.eval(2, 2));
        assert!(Cmp::Lt.eval(1, 2) && !Cmp::Lt.eval(2, 2));
    }
}
