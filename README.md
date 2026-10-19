# API & Webhook Tester

A small cross-platform desktop app written in Rust.

## Features
- Send GET, POST, PUT, PATCH, DELETE, and HEAD requests.
- Add custom request headers and JSON/text request bodies.
- Inspect response status, headers, body, and elapsed time.
- Run a local webhook listener on a configurable port.
- View received webhook request method, path, headers, and payload.
- macOS and Windows release builds via GitHub Actions.

## Build locally
Install Rust from https://rustup.rs, then run:

```bash
cargo run --release
```

Linux users may need GTK/X11/Wayland system development libraries depending on their distribution.

## Build downloadable installers
1. Create a GitHub repository and upload this project.
2. Push a version tag, e.g. `git tag v0.1.0 && git push origin v0.1.0`.
3. GitHub Actions builds release artifacts for macOS Apple Silicon, macOS Intel, and Windows x64.
4. Download `.dmg` and `.exe` from the workflow's GitHub Release.

## Security notes
- This is a development/testing utility, not a production webhook gateway.
- The listener binds to `0.0.0.0`, so it can accept connections from other devices on your network. Only run it on trusted networks and stop it when finished.
- Do not paste production secrets into screenshots or share captured webhook payloads.
- The listener does not verify webhook signatures yet.
