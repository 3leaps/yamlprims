//! Raw input admission and original-byte location scanning.
use crate::budget::limit_err;
use crate::error::internal;
use crate::error::{Error, ErrorKind, LimitKind};
use crate::options::Options;
use crate::value::Location;

/// Gate raw bytes: size, BOM, UTF-8 (§6, §7.2).
pub(crate) fn gate_bytes<'a>(input: &'a [u8], opts: &Options) -> Result<(&'a str, usize), Error> {
    if input.len() > opts.limits.max_input_bytes {
        return Err(limit_err(
            LimitKind::MaxInputBytes,
            opts.limits.max_input_bytes,
            loc0(),
        ));
    }
    let bom = if input.starts_with(b"\xEF\xBB\xBF") {
        3
    } else {
        0
    };
    match std::str::from_utf8(&input[bom..]) {
        Ok(s) => Ok((s, bom)),
        Err(e) => {
            let valid = &input[bom..bom + e.valid_up_to()];
            // Valid prefix by construction.
            let prefix = std::str::from_utf8(valid).map_err(|_| internal("utf8 prefix"))?;
            let loc = scan_location(prefix, bom);
            Err(Error::at(
                ErrorKind::InvalidUtf8 {
                    invalid_len: e.error_len(),
                },
                loc,
                "invalid UTF-8",
            ))
        }
    }
}

pub(crate) fn gate_str<'a>(input: &'a str, opts: &Options) -> Result<(&'a str, usize), Error> {
    if input.len() > opts.limits.max_input_bytes {
        return Err(limit_err(
            LimitKind::MaxInputBytes,
            opts.limits.max_input_bytes,
            loc0(),
        ));
    }
    Ok(match input.strip_prefix('\u{feff}') {
        Some(rest) => (rest, 3),
        None => (input, 0),
    })
}

fn loc0() -> Location {
    Location {
        byte_offset: 0,
        line: 1,
        column: 1,
    }
}

/// End of a valid prefix: LF, CRLF and lone CR are breaks; columns count characters.
pub(crate) fn scan_location(prefix: &str, bom: usize) -> Location {
    let (mut line, mut col) = (1usize, 1usize);
    let mut it = prefix.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\n' => {
                line += 1;
                col = 1
            }
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                line += 1;
                col = 1
            }
            _ => col += 1,
        }
    }
    Location {
        byte_offset: prefix.len() + bom,
        line,
        column: col,
    }
}
