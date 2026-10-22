<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Hosting the web app on Cloudflare

The workflow `.github/workflows/triya-cloudflare.yml` builds the web app on every push to `main`
(and on demand from the Actions tab) and deploys it to Cloudflare Workers:

- `https://triya-cad-3d.<your-subdomain>.workers.dev/` — landing page
- `https://triya-cad-3d.<your-subdomain>.workers.dev/app/` — the app (`?vessel=a` opens vessel A)

It runs beside the GitHub Pages workflow; either can be switched off in Settings → Actions.

## One-time setup (owner)

1. Create a Cloudflare account (the free plan is enough).
2. Cloudflare dashboard → My Profile → API Tokens → Create Token → template
   **Edit Cloudflare Workers** → Create. Copy the token.
3. Note the **Account ID** (dashboard → Workers & Pages, right-hand column).
4. GitHub → repository Settings → Secrets and variables → Actions → New repository secret:
   `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`.
5. Actions → Triya Cloudflare → Run workflow. The first deploy creates the Worker
   `triya-cad-3d`; the log ends with its URL.

Custom domain (optional, the domain's DNS must be on Cloudflare): Workers & Pages →
triya-cad-3d → Settings → Domains & Routes → Add → Custom domain, e.g. `cad.openengtools.in`.
The app then lives at `https://cad.openengtools.in/app/`; nothing in the build changes, because
the app uses relative URLs.

Without the two secrets the workflow still builds and tests the site and skips the deploy.

## Why there is a Worker

Cloudflare refuses any static file over 25 MiB, and the app's WebAssembly is ~41 MiB (~15 MB
gzipped). The workflow uploads `<name>.wasm.gz` in place of `<name>.wasm`. Every other file is
served by Cloudflare directly; a request for `<name>.wasm` matches no file and reaches the Worker
(`triya/cloudflare/src/lib.rs`), which returns the gzip bytes unchanged with
`Content-Encoding: gzip`, and the browser unpacks them. The address stays the same, so offline use
and updates through `sw.js` work as on GitHub Pages.

The Worker is Rust (`worker` crate, MIT OR Apache-2.0), built by `worker-build` at deploy time;
the small JavaScript loader it generates is build output, like Trunk's, and is not committed.
`triya/cloudflare` is its own Cargo workspace, so `cargo xtask ci` does not build it.

## Trying it locally

```sh
cd triya/apps/triya-web && trunk build --release && cd -
mkdir -p site/app && cp -r dist/triya-web/. site/app/ && gzip -9 -n site/app/*.wasm
cd triya/cloudflare && npx wrangler@4 dev
```

Checked 2026-10-22: through `wrangler dev`, the `.wasm` arrives as 14.5 MB gzip, unpacks to the
exact 43.3 MB build, and vessel A's shell regenerates in Chrome in 7.3 s.
