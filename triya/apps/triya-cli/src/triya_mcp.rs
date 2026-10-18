// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The MCP server's stdout, with Triya CAD 3D's name in the `initialize` answer.
//!
//! Upstream's `Server` writes `serverInfo: {name: "solidcraft", title: "SolidCraft"}` from a
//! literal (`crates/mcp/src/server.rs`) and has no parameter for it, so `build.rs` hands
//! `Server::serve` this writer instead of stdout. It passes every line through unchanged except
//! a JSON-RPC response whose `result` carries `serverInfo`, which gets [`triya_brand::MCP_SERVER_NAME`],
//! [`triya_brand::APP_NAME`] and [`triya_brand::VERSION`]. No seam in upstream's MCP crate.
//! Tested through the built binary (`tests/cli.rs`, the crate's unit tests are upstream's and off).

use std::io::Write;

/// Longest line held while waiting for its newline; longer output is passed through as is.
const MAX_LINE: usize = 1 << 20;

/// A line-buffered writer that renames the server in `initialize` responses.
pub struct Renamed<W: Write> {
    inner: W,
    line: Vec<u8>,
}

impl<W: Write> Renamed<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, line: Vec::new() }
    }

    fn emit_line(&mut self) -> std::io::Result<()> {
        let line = std::mem::take(&mut self.line);
        match rename(&line) {
            Some(renamed) => self.inner.write_all(&renamed),
            None => self.inner.write_all(&line),
        }
    }
}

/// The line with Triya's `serverInfo`, or `None` when it is not an answer carrying `serverInfo`.
fn rename(line: &[u8]) -> Option<Vec<u8>> {
    let body = line.strip_suffix(b"\n").unwrap_or(line);
    let body = body.strip_suffix(b"\r").unwrap_or(body);
    let mut msg: serde_json::Value = serde_json::from_slice(body).ok()?;
    let info = msg.get_mut("result")?.get_mut("serverInfo")?.as_object_mut()?;
    info.insert("name".into(), triya_brand::MCP_SERVER_NAME.into());
    info.insert("title".into(), triya_brand::APP_NAME.into());
    info.insert("version".into(), triya_brand::VERSION.into());
    let mut out = serde_json::to_vec(&msg).ok()?;
    out.extend_from_slice(&line[body.len()..]);
    Some(out)
}

impl<W: Write> Write for Renamed<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for chunk in buf.split_inclusive(|&b| b == b'\n') {
            self.line.extend_from_slice(chunk);
            if chunk.ends_with(b"\n") || self.line.len() > MAX_LINE {
                self.emit_line()?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Messages are newline-terminated; a partial line stays until its end arrives.
        self.inner.flush()
    }
}

impl<W: Write> Drop for Renamed<W> {
    fn drop(&mut self) {
        if !self.line.is_empty() {
            let _ = self.emit_line();
            let _ = self.inner.flush();
        }
    }
}
