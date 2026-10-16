//! hae-jsonl: JSON-lines transport over stdio for the session executor (spec §6, §11).
//!
//! The server writes one `hello` line (sessionId, writerId), then answers each
//! request line with exactly one response line. The client fills
//! `writerId` and `requestSequence`; the executor enforces ordering, replay and
//! conflict rules. Requests over 1MiB and responses over 4MiB are refused with
//! `RESOURCE_LIMIT` without consuming a sequence number (an oversize request is
//! never forwarded). Diagnostics logs go to stderr only.

use hae_diag::{Code, Diagnostic, Stage};
use hae_exec::{RequestEnvelope, ResponseEnvelope, SessionExecutor};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

/// Largest accepted request line, excluding the newline (spec §11).
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
/// Largest JSON response line, excluding the newline (spec §11).
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// One line read from the transport.
#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    /// A complete line, newline and trailing `\r` removed.
    Text(Vec<u8>),
    /// A line longer than the limit; the whole line was consumed and dropped.
    TooLong {
        bytes: usize,
    },
    Eof,
}

/// Read one line holding at most `limit` bytes in memory. Longer lines are
/// drained to their newline so the next read starts at the next request.
pub fn read_line_limited<R: BufRead>(reader: &mut R, limit: usize) -> io::Result<Line> {
    let mut buf: Vec<u8> = Vec::new();
    let mut total = 0usize;
    let mut saw_any = false;
    loop {
        let chunk = match reader.fill_buf() {
            Ok(chunk) => chunk,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if chunk.is_empty() {
            if !saw_any {
                return Ok(Line::Eof);
            }
            break;
        }
        saw_any = true;
        let (take, found_newline) = match chunk.iter().position(|b| *b == b'\n') {
            Some(at) => (at, true),
            None => (chunk.len(), false),
        };
        total += take;
        if total <= limit {
            buf.extend_from_slice(&chunk[..take]);
        }
        let consume = if found_newline { take + 1 } else { take };
        reader.consume(consume);
        if found_newline {
            break;
        }
    }
    if total > limit {
        return Ok(Line::TooLong { bytes: total });
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(Line::Text(buf))
}

fn error_response(
    session_id: &str,
    request_id: Option<String>,
    command: &str,
    diagnostic: Diagnostic,
) -> ResponseEnvelope {
    ResponseEnvelope {
        schema_version: 1,
        session_id: session_id.to_string(),
        request_id,
        command: command.to_string(),
        ok: false,
        replayed: false,
        result: None,
        diagnostics: vec![diagnostic],
    }
}

/// Response replacing one that exceeds the 4MiB limit.
pub fn response_too_large(response: &ResponseEnvelope, bytes: usize) -> ResponseEnvelope {
    error_response(
        &response.session_id,
        response.request_id.clone(),
        &response.command,
        Diagnostic::error(
            Code::ResourceLimit,
            Stage::Request,
            format!("response of {bytes} bytes exceeds the {MAX_RESPONSE_BYTES} byte limit; use cursors to page large results"),
        )
        .with_reason("response_too_large")
        .with_detail(json!({ "limitBytes": MAX_RESPONSE_BYTES, "responseBytes": bytes })),
    )
}

/// Diagnostic for a request line over the 1MiB limit.
pub fn request_too_large(bytes: usize) -> Diagnostic {
    Diagnostic::error(
        Code::ResourceLimit,
        Stage::Request,
        format!("request line of {bytes} bytes exceeds the {MAX_REQUEST_BYTES} byte limit"),
    )
    .with_reason("request_too_large")
    .with_detail(json!({ "limitBytes": MAX_REQUEST_BYTES, "requestBytes": bytes }))
}

/// Serialize a response, swapping in a `RESOURCE_LIMIT` error when it is too large.
pub fn encode_response(response: &ResponseEnvelope) -> String {
    let text = serde_json::to_string(response).unwrap_or_default();
    if text.len() <= MAX_RESPONSE_BYTES {
        return text;
    }
    let replacement = response_too_large(response, text.len());
    serde_json::to_string(&replacement).unwrap_or_default()
}

/// The line the server writes on connect.
pub fn hello_line(session_id: &str, writer_id: &str) -> String {
    json!({
        "type": "hello",
        "schemaVersion": 1,
        "sessionId": session_id,
        "writerId": writer_id,
    })
    .to_string()
}

/// Handle one request line (already within the size limit) and build its response.
pub fn handle_line(executor: &mut SessionExecutor, line: &[u8]) -> ResponseEnvelope {
    let session_id = executor.session_id().to_string();
    let value: Value = match serde_json::from_slice(line) {
        Ok(value) => value,
        Err(e) => {
            return error_response(
                &session_id,
                None,
                "",
                Diagnostic::error(
                    Code::InvalidRequest,
                    Stage::Request,
                    format!("request line is not valid JSON: {e}"),
                )
                .with_reason("malformed_json"),
            );
        }
    };
    let request_id = value
        .get("requestId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let command = value
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // Malformed envelopes never reach the executor, so they cannot consume a
    // sequence number (spec §6: writer and number unknown).
    let request: RequestEnvelope = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(e) => {
            return error_response(
                &session_id,
                request_id,
                &command,
                Diagnostic::error(
                    Code::InvalidRequest,
                    Stage::Request,
                    format!("invalid request envelope: {e}"),
                ),
            );
        }
    };
    executor.execute(request)
}

fn write_line<W: Write>(out: &mut W, text: &str) -> io::Result<()> {
    out.write_all(text.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

fn log_response(response: &ResponseEnvelope) {
    if response.ok {
        eprintln!(
            "hae-jsonl: {} ok{}",
            response.command,
            if response.replayed { " (replay)" } else { "" }
        );
    } else {
        for diagnostic in &response.diagnostics {
            eprintln!(
                "hae-jsonl: {} failed: {} ({})",
                response.command, diagnostic.code, diagnostic.message
            );
        }
    }
}

/// Serve requests until EOF. Returns `Ok(())` on a clean end of input.
pub fn serve<R: BufRead, W: Write>(
    executor: &mut SessionExecutor,
    reader: &mut R,
    out: &mut W,
) -> io::Result<()> {
    write_line(
        out,
        &hello_line(executor.session_id(), executor.default_writer_id()),
    )?;
    loop {
        let response = match read_line_limited(reader, MAX_REQUEST_BYTES)? {
            Line::Eof => return Ok(()),
            Line::TooLong { bytes } => {
                error_response(executor.session_id(), None, "", request_too_large(bytes))
            }
            Line::Text(bytes) => {
                if bytes.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                handle_line(executor, &bytes)
            }
        };
        log_response(&response);
        write_line(out, &encode_response(&response))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn line_reader_splits_trims_and_drains_oversize_lines() {
        let mut input = Cursor::new(b"abc\r\n\nsmall\n".to_vec());
        assert_eq!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::Text(b"abc".to_vec())
        );
        assert_eq!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::Text(Vec::new())
        );
        assert_eq!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::Text(b"small".to_vec())
        );
        assert_eq!(read_line_limited(&mut input, 10).unwrap(), Line::Eof);

        let mut input = Cursor::new(b"0123456789AB\nnext\ntail".to_vec());
        assert_eq!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::TooLong { bytes: 12 }
        );
        assert_eq!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::Text(b"next".to_vec())
        );
        // A final line without newline is still a line.
        assert_eq!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::Text(b"tail".to_vec())
        );
        assert_eq!(read_line_limited(&mut input, 10).unwrap(), Line::Eof);

        // Exactly at the limit is accepted; one over is not.
        let mut input = Cursor::new(b"0123456789\n01234567890\n".to_vec());
        assert!(matches!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::Text(t) if t.len() == 10
        ));
        assert_eq!(
            read_line_limited(&mut input, 10).unwrap(),
            Line::TooLong { bytes: 11 }
        );
    }

    #[test]
    fn oversize_response_is_replaced_by_resource_limit_error() {
        let big = ResponseEnvelope {
            schema_version: 1,
            session_id: "ses-x".into(),
            request_id: Some("req-1".into()),
            command: "document.inspect".into(),
            ok: true,
            replayed: false,
            result: Some(json!({ "blob": "x".repeat(MAX_RESPONSE_BYTES + 1) })),
            diagnostics: Vec::new(),
        };
        let text = encode_response(&big);
        assert!(text.len() < 4096);
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["ok"], json!(false));
        assert_eq!(value["requestId"], json!("req-1"));
        assert_eq!(value["command"], json!("document.inspect"));
        assert_eq!(value["diagnostics"][0]["code"], json!("RESOURCE_LIMIT"));

        let small = ResponseEnvelope {
            result: Some(json!({ "ok": 1 })),
            ..big
        };
        assert!(serde_json::from_str::<Value>(&encode_response(&small)).unwrap()["ok"] == true);
    }
}
