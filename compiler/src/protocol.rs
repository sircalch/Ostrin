//! Minimal `Content-Length`-framed JSON transport shared by the two stdio
//! protocols this compiler speaks: the Language Server Protocol (`lsp.rs`)
//! and the Debug Adapter Protocol (`dap.rs`, plus the interpreter's own
//! pause loop in `interpreter/mod.rs` while a breakpoint is hit). Both
//! protocols use the exact same header/body framing; only the JSON message
//! shape inside differs, which is each protocol's own concern.

use std::io::{self, BufRead, Write};

use serde_json::Value;

pub fn read_message<R: BufRead + ?Sized>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            return Ok(None);
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = Some(value.trim().parse::<usize>().map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid Content-Length: {error}"),
                    )
                })?);
            }
        }
    }
    let length = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "message has no Content-Length header",
        )
    })?;
    let mut body = vec![0_u8; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

pub fn write_message<W: Write + ?Sized>(writer: &mut W, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}
