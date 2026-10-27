# Windows Web Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development or executing-plans, with independent frontend/API tasks and review before completion.

**Goal:** Deliver native Windows Rust API plus static browser UI for existing WMS operations.
**Architecture:** Reuse Rust services via a library and headless API binary; static-export current browser UI, served through a conventional web server. Preserve local egui.
**Tech Stack:** Rust, SQLite, HTTP router, existing React/Next static build, Caddy deployment example.

- [x] Add API integration tests for authentication, forced password changes, role enforcement, stock operations, session invalidation, and upload path rejection. Observe failing tests before implementation.
- [x] Expose Rust services as library, add optional desktop dependencies and headless API executable; implement auth/session lifecycle and errors.
- [x] Implement API adapters for item/lot/scan/dictionary/user/role/settings/camera/recognition operations, reusing service validation and transactions. Test HTTP contracts against the browser client.
- [x] Export frontend statically, repair parity gaps and authenticated images, verify production build and routes.
- [x] Add Windows configuration/start scripts and release artifacts containing API binary and static assets. Document first-run login and deployment.
- [x] Run cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace, frontend production build, API smoke and browser verification. Review diff; report actual coverage and Windows limitations.


## Verification record

- Synced upstream `manibari/Norns-Urd-WMS` main to `1dfce02` before implementation; work on `feat/windows-static-web`.
- `cargo test --workspace`: 121 tests passed (40 application, 1 CLI, 6 HTTP integration, 74 core).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo clippy -p urdwms --no-default-features --features server --all-targets -- -D warnings`: passed; headless server builds independently of desktop UI.
- `cargo fmt --all --check` and `git diff --check`: passed.
- Frontend static export and 3 API client tests passed; actual Caddy 2.11.7 static routes, proxy, authentication and session invalidation verified locally.
- Real browser receiving, manual FIFO issuing, scan detail edits persisted across reload, API-key save/clear, alert settings and password changes preserving the current session verified against Rust API.
- Independent review found and resolved session resurrection race, rejected-stock override, cross-item scan payloads, and unbounded history reads.
- Windows script syntax/headless smoke are configured in CI but not executed locally (Linux host). Physical camera, live Gemini service, Surface memory/performance, and Windows launcher lifecycle remain device/environment validation.
- Existing egui stays a local application using shared services; browser is the remote client in this delivery. No remote egui transport or Windows service installation is included.

- Final browser correction: sidebar retains reachable logout at 493px viewport height (y376 after fix), verified actual logout; frontend rebuilt successfully after this CSS correction. Test API/Caddy/Python processes stopped.
