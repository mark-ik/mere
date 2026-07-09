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
        let content = match raw.split('#').next() {
            Some(c) => c.trim(),
            None => "",
        };
        if content.is_empty() {
            continue;
        }
        steps.push(parse_line(content, line)?);
    }
    Ok(steps)
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
    fn cmp_evaluates_each_operator() {
        assert!(Cmp::Eq.eval(2, 2));
        assert!(Cmp::Ne.eval(1, 2));
        assert!(Cmp::Ge.eval(2, 2) && Cmp::Ge.eval(3, 2));
        assert!(Cmp::Le.eval(2, 2) && Cmp::Le.eval(1, 2));
        assert!(Cmp::Gt.eval(3, 2) && !Cmp::Gt.eval(2, 2));
        assert!(Cmp::Lt.eval(1, 2) && !Cmp::Lt.eval(2, 2));
    }
}
