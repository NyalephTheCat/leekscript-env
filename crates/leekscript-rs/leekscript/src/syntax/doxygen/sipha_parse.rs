//! Sipha-backed tokenization of Doxygen bodies into [`super::scan::Segment`]s.
//!
//! The grammar recognizes `\name` / `@name` commands (with `\\` and `\@` escapes in prose) and
//! [`super::commands::apply_segments`] lowers the result to [`super::model::ParsedDoxygen`].

use std::sync::OnceLock;

use sipha::SyntaxKinds;
use sipha::diagnostics::parsed_doc::ParsedDoc;
use sipha::parse::engine::Engine;
use sipha::prelude::*;
use sipha::tree::red::{SyntaxElement, SyntaxNode};

use super::scan::Segment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, SyntaxKinds)]
#[repr(u16)]
enum Dk {
    Root,
    Command,
    SlashCmd,
    AtCmd,
    TextRun,
    Ws,
}

static DOXYGEN_GRAPH: OnceLock<BuiltGraph> = OnceLock::new();

fn doxygen_graph() -> &'static BuiltGraph {
    DOXYGEN_GRAPH.get_or_init(build_doxygen_graph)
}

fn build_doxygen_graph() -> BuiltGraph {
    let mut g = GrammarBuilder::new();
    g.set_trivia_rule("doc_ws");

    // Match Leek's `trivia` lexer rule: optional skip so parser `skip()` succeeds at column 0
    // without consuming (see `grammar/tokens.rs` `lexer_rule("trivia", ...)`).
    g.lexer_rule("doc_ws", |g| {
        g.optional(|g| {
            g.trivia(Dk::Ws, |g| {
                g.one_or_more(|g| {
                    g.class(classes::WHITESPACE);
                });
            });
        });
    });

    let cmd_ident = |g: &mut GrammarBuilder| {
        g.class(classes::IDENT_START);
        g.zero_or_more(|g| {
            g.class(classes::IDENT_CONT);
        });
    };

    g.lexer_rule("slash_cmd_tok", |g| {
        g.token(Dk::SlashCmd, |g| {
            g.byte(b'\\');
            g.neg_lookahead(|g| {
                g.byte(b'\\');
            });
            g.neg_lookahead(|g| {
                g.byte(b'@');
            });
            cmd_ident(g);
        });
    });

    g.lexer_rule("at_cmd_tok", |g| {
        g.token(Dk::AtCmd, |g| {
            g.byte(b'@');
            cmd_ident(g);
        });
    });

    g.lexer_rule("text_run_tok", |g| {
        g.token(Dk::TextRun, |g| {
            g.one_or_more(|g| {
                sipha::choices!(
                    g,
                    |g| {
                        g.literal(b"\\\\");
                    },
                    |g| {
                        g.literal(b"\\@");
                    },
                    |g| {
                        g.neg_lookahead(|g| {
                            g.byte(b'\\');
                            g.class(classes::IDENT_START);
                        });
                        g.neg_lookahead(|g| {
                            g.byte(b'@');
                            g.class(classes::IDENT_START);
                        });
                        g.any_char();
                    },
                );
            });
        });
    });

    g.parser_rule("cmd_elt", |g| {
        g.node(Dk::Command, |g| {
            g.choice(
                |g| {
                    g.call("slash_cmd_tok");
                },
                |g| {
                    g.call("at_cmd_tok");
                },
            );
            g.optional(|g| {
                g.call("text_run_tok");
            });
        });
    });

    g.parser_rule("start", |g| {
        g.node(Dk::Root, |g| {
            g.zero_or_more(|g| {
                g.choice(
                    |g| {
                        g.call("cmd_elt");
                    },
                    |g| {
                        g.call("text_run_tok");
                    },
                );
            });
        });
        g.end_of_input();
        g.accept();
    });

    g.finish().expect("doxygen sipha grammar")
}

/// Parse `body` with sipha and split into the same [`Segment`] shape as [`super::scan::split_command_segments`].
///
/// Returns [`None`] if the sipha parse fails.
pub(crate) fn split_via_sipha(body: &str) -> Option<Vec<Segment>> {
    let built = doxygen_graph();
    let graph = built.as_graph();
    let mut engine = Engine::new();
    let bytes = body.as_bytes();
    let out = engine.parse_rule_named(&graph, bytes, "start").ok()?;
    let doc = ParsedDoc::from_slice(bytes, &out)?;
    tree_root_to_segments(doc.root())
}

fn tree_root_to_segments(root: &SyntaxNode) -> Option<Vec<Segment>> {
    if root.kind_as::<Dk>() != Some(Dk::Root) {
        return None;
    }
    let mut segments: Vec<Segment> = Vec::new();
    let mut buf = String::new();

    for el in root.children() {
        match &el {
            SyntaxElement::Token(t) => {
                if t.kind_as::<Dk>() == Some(Dk::TextRun) {
                    buf.push_str(t.text());
                } else if t.is_trivia() {
                    buf.push_str(t.text());
                }
            }
            SyntaxElement::Node(n) => {
                if n.kind_as::<Dk>() == Some(Dk::Command) {
                    if !buf.is_empty() {
                        segments.push(Segment::Leading(std::mem::take(&mut buf)));
                    }
                    segments.push(command_node_to_segment(n)?);
                }
            }
        }
    }

    if !buf.is_empty() {
        segments.push(Segment::Leading(buf));
    }

    Some(segments)
}

fn command_node_to_segment(cmd: &SyntaxNode) -> Option<Segment> {
    let mut name: Option<String> = None;
    let mut arg = String::new();
    for el in cmd.children() {
        match &el {
            SyntaxElement::Token(t) => {
                if t.kind_as::<Dk>() == Some(Dk::SlashCmd) {
                    name = Some(cmd_name_from_token(t.text(), b'\\')?);
                } else if t.kind_as::<Dk>() == Some(Dk::AtCmd) {
                    name = Some(cmd_name_from_token(t.text(), b'@')?);
                } else if t.kind_as::<Dk>() == Some(Dk::TextRun) {
                    arg.push_str(t.text());
                } else if t.is_trivia() {
                    arg.push_str(t.text());
                }
            }
            SyntaxElement::Node(_) => {}
        }
    }
    Some(Segment::Cmd {
        name: name?,
        arg: super::scan::clean_arg_text(&arg),
    })
}

fn cmd_name_from_token(text: &str, trigger: u8) -> Option<String> {
    let b = text.as_bytes();
    if b.first().copied() != Some(trigger) {
        return None;
    }
    Some(text[1..].to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::super::scan::split_command_segments;
    use super::*;

    #[test]
    fn sipha_matches_scanner() {
        let samples = [
            r"\brief Hi \param x y \return z",
            "Leading text \\brief not a cmd\n\\brief real",
            "@brief At-style\n@details More",
        ];
        for s in samples {
            let a = split_via_sipha(s).expect("sipha parse");
            let b = split_command_segments(s);
            assert_eq!(a, b, "mismatch for {s:?}");
        }
    }
}
