// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors

//! Serves the web app's WebAssembly from a gzip copy.
//!
//! Cloudflare rejects static files over 25 MiB and the app's `.wasm` is ~41 MiB, so the deploy
//! workflow uploads `<name>.wasm.gz` (~15 MB) instead. Every other file is served by Cloudflare
//! without running this Worker; a request for `<name>.wasm` matches no file, so it lands here and
//! gets the gzip bytes as they are, labelled `Content-Encoding: gzip`, which the browser unpacks.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use worker::{EncodeBody, Env, Headers, Request, Response, ResponseBuilder, Result, event};

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    let assets = env.assets("ASSETS")?;
    let url = req.url()?;
    if !url.path().ends_with(".wasm") {
        return assets.fetch_request(req).await;
    }
    let mut gz = url.clone();
    gz.set_path(&format!("{}.gz", url.path()));
    let found = assets.fetch(gz.to_string(), None).await?;
    if found.status_code() != 200 {
        return Response::error("Not found", 404);
    }
    let (_, body) = found.into_parts();
    let headers = Headers::new();
    headers.set("Content-Type", "application/wasm")?;
    headers.set("Content-Encoding", "gzip")?;
    // Trunk puts a content hash in the file name, so a name never changes meaning.
    headers.set("Cache-Control", "public, max-age=31536000, immutable")?;
    Ok(ResponseBuilder::new().with_headers(headers).with_encode_body(EncodeBody::Manual).body(body))
}
