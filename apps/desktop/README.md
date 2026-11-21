# Screenstory Desktop Prototype

Tauri v2 + React + TypeScript + Rust foundation.

## Scope

This prototype implements only:
- A Tauri desktop shell.
- A React empty-state dashboard.
- A minimal typed Rust app-info command.
- Least-privilege default Tauri capabilities.
- Frontend unit tests and Linux CI packaging.

It does not implement project creation, screen capture, AI analysis, timeline editing, or export.

## Development

Development dependencies are documented in the official Tauri v2 prerequisites. End-user packages are built in CI; end users do not need Rust, Node.js, or build headers.

From this directory:
- `pnpm install`
- `pnpm tauri dev`
- `pnpm typecheck`
- `pnpm test`
- `pnpm build`
- `pnpm tauri build --bundles deb`

The Debian package declares the GTK and WebKitGTK runtime libraries required to launch the application. Feature-specific runtime dependencies such as FFmpeg will be checked and handled by the application in a later approved task.

## Status

Prototype scaffold only. A successful CI run and generated package inspection are required before calling this prototype build-verified.
