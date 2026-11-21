# SPEC-024 — User-Consented Runtime Dependency Installation

**Status:** Approved for implementation after M7 capture integration  
**Milestone:** M8

## 1. Product requirement

End users must not install Node.js, pnpm, Rust, compilers, headers, Python, or build tools to run Screenstory. These are build/CI-only dependencies. The Debian package declares mandatory desktop libraries so the system package manager resolves them before the app launches.

Optional feature dependencies are detected by Screenstory and offered only when the user invokes the feature that needs them.

## 2. Runtime dependency inventory

- Recording and local frame extraction: FFmpeg plus FFprobe.
- Zoom/video composition and caption burn-in: FFmpeg filters `zoompan` and `ass` (libass), encoder `libx264`, and AAC encoder support.
- Arabic caption rendering: Fontconfig and a verified Arabic-capable font/shaping environment; the app must block Arabic burn-in when verification fails.
- Wayland recording: XDG Desktop Portal ScreenCast service and PipeWire runtime components. These are session/environment prerequisites and may not be safely installable by a generic APT package action.

Each capability reports `available`, `missing`, `unsupported`, or `failed`, with the missing package list and a human-readable reason.

## 3. Consent-first installation

- Detect exact binaries, FFmpeg filters/encoders and system services at startup and immediately before use.
- When a feature is invoked and a missing APT-managed dependency can be installed safely, show the package names, purpose, approximate package manager action, and request explicit user consent.
- No installation is performed before consent.
- No third-party PPAs, repositories, signing keys, curl-pipe-shell, or arbitrary user-provided package names.
- The installer uses a fixed allowlist of distribution package names and fixed package-manager argument vectors; no shell interpolation.
- Use PolicyKit authorization with a graphical system prompt. If the prompt is unavailable/cancelled, return a clear status without closing Screenstory.
- Stream real package-manager output to the UI as progress; do not synthesize percentage progress.
- Handle package-manager locks, network failure, unavailable package, authorization cancellation, partial installation, and post-install verification.
- Recheck the requested capability after installation before enabling the feature.

## 4. Failure isolation

- Declining or failing installation disables only the requested feature.
- Project browsing, timeline editing and any unrelated available features remain usable.
- The UI reports what is unavailable and how to retry from the feature screen.
- Never claim a feature is ready based solely on package-manager exit status; run the actual binary/filter/encoder probes.

## 5. Debian and build policy

- Build-time dependencies remain solely in CI/build images.
- Tauri mandatory GTK/WebKit/D-Bus libraries remain in `.deb` Depends metadata.
- Validate dependency names against the supported Linux Mint 21.3+ and 22.x Ubuntu bases.
- The package must not install packages through maintainer scripts or silently alter user repositories.
- Primary release format remains `.deb`; Flatpak is deferred.

## 6. Acceptance criteria

1. Clean supported Linux Mint installation opens Screenstory without manual developer tool setup.
2. Mandatory desktop runtime dependencies are resolved by APT before app launch.
3. Missing FFmpeg is detected on feature invocation and a consent dialog accurately lists the fixed packages to install.
4. Decline/cancel/PolicyKit failure leaves the app open and other features usable.
5. Approved package install shows real progress/output and rechecks FFmpeg + FFprobe.
6. Missing libass/libx264 or Arabic font/shaping disables only the corresponding rendering capability.
7. No arbitrary command, package string, PPA, external repository or signing key can enter the installer.
8. Package-manager locks, network failures and partial installs produce recoverable UI states.
9. CI validates package metadata on supported Ubuntu bases; release QA validates actual Linux Mint versions.
