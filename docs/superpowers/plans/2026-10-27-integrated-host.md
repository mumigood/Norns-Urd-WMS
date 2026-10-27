# Integrated Host Implementation Plan

> Execute with subagent-driven-development, independent migration and packaging workers, then review and integrate.

**Goal:** One Windows installation with shared egui/web data and automatic safe upgrade from original desktop DB.
**Architecture:** One launcher, shared original data directory, common Rust services and SQLite migrations, egui external-change detection.

- [x] Add regression tests for legacy DB/WAL migration, preservation, idempotency and future-version refusal; implement atomic backed-up migration.
- [x] Centralize default data directory and reuse in desktop/API; preserve URDWMS_DATA.
- [x] Test and implement PRAGMA data_version polling for egui external changes and account refresh.
- [x] Add integrated Windows launcher and combined CI/release package/documentation.
- [x] Run Rust tests, clippy, format; HTTP integration verifies old data through API and shared connection writes.
- [x] Cross-compile both binaries, assemble combined Windows test ZIP, validate dependencies and package integrity.


## Verification

- `cargo test --workspace`: 130 tests passed (47 shared application/migration, 1 CLI, 2 legacy/shared-data HTTP integration, 6 HTTP contracts, 74 core).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- SQLite fixtures verify committed WAL backup, all account hashes/settings/uploads retained, missing historical columns, rollback on failed unique index, backup failure, corrupt/future DB refusal, idempotency, and bidirectional egui-service/API changes.
- Desktop external refresh defers Settings/Basics/Issue reloads to preserve unsaved inputs and selected batches; read-only lists refresh using data_version. External credential/activation changes invalidate the local signed-in session.
- Both Windows x64 Release binaries cross-compiled using LLVM-MinGW UCRT. Combined test package includes static frontend, pinned official Caddy2.11.7, one-click launcher, runtime DLLs and licenses.
- Native Windows launcher/OpenGL/Surface testing and the user's actual legacy database remain unverified; synthetic representative old egui fixtures passed.
