//! Map segmented commands onto [`super::model::ParsedDoxygen`].

use super::model::{DoxygenParam, DoxygenRetval, DoxygenThrows, ParsedDoxygen};
use super::scan::Segment;

pub(crate) fn apply_segments(segments: Vec<Segment>, raw: String) -> ParsedDoxygen {
    let mut leading = String::new();
    let mut cmds: Vec<(String, String)> = Vec::new();
    for seg in segments {
        match seg {
            Segment::Leading(s) => leading = s,
            Segment::Cmd { name, arg } => cmds.push((name, arg)),
        }
    }

    let leading = leading.trim().to_string();
    let mut out = ParsedDoxygen {
        raw,
        ..ParsedDoxygen::default()
    };

    for (name, arg) in cmds {
        let arg = arg.trim();
        match name.as_str() {
            "brief" | "short" => {
                if out.brief.is_none() {
                    out.brief = Some(arg.to_string());
                }
            }
            "details" => {
                append_opt(&mut out.details, arg);
            }
            "param" | "arg" => {
                if let Some(p) = parse_param_arg(arg) {
                    out.params.push(p);
                }
            }
            "tparam" | "template" => {
                if let Some(p) = parse_param_arg(arg) {
                    out.template_params.push(p);
                }
            }
            "return" | "returns" | "result" => {
                if out.returns.is_none() {
                    out.returns = Some(arg.to_string());
                }
            }
            "retval" => {
                if let Some(r) = parse_retval_arg(arg) {
                    out.retvals.push(r);
                }
            }
            "see" | "sa" | "ref" | "cite" => {
                if !arg.is_empty() {
                    out.see_also.push(arg.to_string());
                }
            }
            "deprecated" => {
                if out.deprecated.is_none() {
                    out.deprecated = Some(arg.to_string());
                }
            }
            "note" => {
                append_opt(&mut out.note, arg);
            }
            "warning" | "warn" => {
                append_opt(&mut out.warning, arg);
            }
            "attention" => {
                append_opt(&mut out.attention, arg);
            }
            "pre" => {
                append_opt(&mut out.preconditions, arg);
            }
            "post" => {
                append_opt(&mut out.postconditions, arg);
            }
            "invariant" => {
                append_opt(&mut out.invariant, arg);
            }
            "remark" | "remarks" => {
                append_opt(&mut out.remark, arg);
            }
            "par" => {
                append_opt(&mut out.details, arg);
            }
            "throws" | "throw" | "exception" => {
                out.throws.push(parse_throws_arg(arg));
            }
            "since" => {
                if out.since.is_none() {
                    out.since = Some(arg.to_string());
                }
            }
            "author" => {
                if !arg.is_empty() {
                    out.authors.push(arg.to_string());
                }
            }
            "version" => {
                if out.version.is_none() {
                    out.version = Some(arg.to_string());
                }
            }
            "copyright" => {
                if out.copyright.is_none() {
                    out.copyright = Some(arg.to_string());
                }
            }
            "bug" => {
                if !arg.is_empty() {
                    out.bugs.push(arg.to_string());
                }
            }
            "todo" => {
                if !arg.is_empty() {
                    out.todos.push(arg.to_string());
                }
            }
            "test" => {
                if !arg.is_empty() {
                    out.tests.push(arg.to_string());
                }
            }
            "internal" => {
                out.internal = true;
            }
            "overload" => {
                out.overload = true;
            }
            _ => {
                out.unknown.push((name, arg.to_string()));
            }
        }
    }

    if out.brief.is_none() && !leading.is_empty() {
        out.brief = Some(leading);
    } else if out.brief.is_some() && !leading.is_empty() {
        prepend_opt(&mut out.details, &leading);
    }

    out
}

fn append_opt(target: &mut Option<String>, piece: &str) {
    match target {
        None => *target = Some(piece.to_string()),
        Some(s) => {
            if !s.is_empty() {
                s.push('\n');
            }
            s.push_str(piece);
        }
    }
}

fn prepend_opt(target: &mut Option<String>, piece: &str) {
    match target {
        None => *target = Some(piece.to_string()),
        Some(s) => {
            if s.is_empty() {
                *s = piece.to_string();
            } else {
                let mut n = piece.to_string();
                n.push('\n');
                n.push_str(s);
                *s = n;
            }
        }
    }
}

fn parse_param_arg(arg: &str) -> Option<DoxygenParam> {
    let mut rest = arg.trim();
    let mut direction = None;
    if let Some(stripped) = rest.strip_prefix('[') {
        if let Some(end) = stripped.find(']') {
            direction = Some(stripped[..end].trim().to_string());
            rest = stripped[end + 1..].trim_start();
        }
    }
    let mut it = rest.split_whitespace();
    let name = it.next()?.to_string();
    let description = it.collect::<Vec<_>>().join(" ");
    Some(DoxygenParam {
        direction,
        name,
        description,
    })
}

fn parse_throws_arg(arg: &str) -> DoxygenThrows {
    let arg = arg.trim();
    let mut it = arg.splitn(2, char::is_whitespace);
    let first = it.next().unwrap_or("");
    let second = it.next();
    match second {
        None => DoxygenThrows {
            type_name: None,
            description: first.to_string(),
        },
        Some(desc) => DoxygenThrows {
            type_name: Some(first.to_string()),
            description: desc.trim().to_string(),
        },
    }
}

fn parse_retval_arg(arg: &str) -> Option<DoxygenRetval> {
    let mut it = arg.split_whitespace();
    let value = it.next()?.to_string();
    let description = it.collect::<Vec<_>>().join(" ");
    Some(DoxygenRetval { value, description })
}
