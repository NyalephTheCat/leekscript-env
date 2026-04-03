//! Manual scan fallback: split a Doxygen body into leading prose and `\` / `@` commands.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    Leading(String),
    Cmd { name: String, arg: String },
}

/// Split `s` into leading prose (before first command) and (`command`,`argument`) pairs.
pub(crate) fn split_command_segments(s: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let Some((cmd_pos, mut current_name, after_name)) = find_next_command(s, 0) else {
        out.push(Segment::Leading(s.to_string()));
        return out;
    };

    if cmd_pos > 0 {
        out.push(Segment::Leading(s[..cmd_pos].to_string()));
    }

    let mut pos = skip_arg_start(s, after_name);
    loop {
        let next_cmd = find_next_command(s, pos);
        let arg_end = next_cmd.as_ref().map(|(i, _, _)| *i).unwrap_or(s.len());
        let arg = clean_arg_text(s.get(pos..arg_end).unwrap_or(""));
        out.push(Segment::Cmd {
            name: current_name,
            arg,
        });
        match next_cmd {
            None => break,
            Some((_, n, na)) => {
                current_name = n;
                pos = skip_arg_start(s, na);
            }
        }
    }

    out
}

pub(crate) fn skip_arg_start(s: &str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'*' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    i
}

pub(crate) fn clean_arg_text(t: &str) -> String {
    t.lines()
        .map(|line| {
            let x = line.trim();
            x.strip_prefix('*').map_or(x, str::trim).to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// `(index_of_trigger, command_name_lowercase, index_after_command_name)`.
pub(crate) fn find_next_command(s: &str, from: usize) -> Option<(usize, String, usize)> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' || b == b'@' {
            if b == b'\\' && i + 1 < bytes.len() {
                let n = bytes[i + 1];
                if n == b'\\' || n == b'@' {
                    i += 2;
                    continue;
                }
            }
            if b == b'@' && i > 0 && bytes[i - 1] == b'\\' {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            if j == i + 1 {
                i += 1;
                continue;
            }
            let name = s[i + 1..j].to_lowercase();
            return Some((i, name, j));
        }
        i += 1;
    }
    None
}
