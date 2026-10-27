# Windows 11 static web + Rust API

User approved this architecture in conversation and requested implementation. Target: 8GB Surface, native Windows 11; no WSL, Python or Node runtime on deployment. A conventional web server serves prebuilt static pages and proxies same-origin /api and /uploads to a Rust API. Network forwarding belongs to operations, not application business logic.

Reuse the existing Rust service layer (permissions, transactions, FIFO, audit, settings and recognition). Add a headless API executable sharing that layer. Keep egui local mode working against the same rules and database schema. Remote operation in this delivery is through the browser; converting all egui calls into a network client is a separate follow-up, not necessary for browser access.

Adapt the retained frontend into a static export, align missing features with egui, and preserve authenticated image access. HTTP sessions are random, expire, are invalidated on logout and password changes, and re-read account status/permissions. Force first-login password changes. Bound uploads and protect paths. SQLite stays on the host, using transactions for competing writes. Slow camera/recognition work must not hold a global database lock.

Ship Windows launch/configuration examples for a conventional static server (Caddy), configurable API listen address and data directory, build/release packaging, and deployment instructions. Do not change host firewall, port forwarding or live production data. Validate API behavior, frontend static build, Rust formatting/lint/tests, and browser flows. Windows execution itself must be checked on Windows CI and explicitly distinguished from local Linux verification.
