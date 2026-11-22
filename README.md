# Afterlife

A local-first autonomous workflow control plane built in pure Rust with [GPUI](https://gpui.rs) and Metal, structurally separating AI execution (`Actor::Agent`) from human governance (`Actor::Human`).

## Overview

- **Fail-Closed Governance Boundary:** AI agents autonomously queue, claim (`fencing_token`), advance sub-stages, attach artifacts, record gate verifications, and propose versioned `Philosophy` / `ProcedureSpec` updates—while human `ApprovalGate` boundaries and governance approvals remain strictly enforced across authentication, the domain engine, and persistence.
- **Hexagonal Local-First Architecture:** `afterlife-core` has zero I/O dependencies. The same domain engine, REST/SSE API, and Streamable HTTP MCP server run locally (SQLite WAL + local artifacts) or on cloud infrastructure behind swappable port traits. See [`docs/architecture.md`](docs/architecture.md) for protocol, concurrency, and deployment details.
- **Native macOS Desktop App (`Afterlife.app`):** Custom GPUI + Metal renderer featuring Backdrop-Synced Liquid Glass, tiled `ParticleWave` compute shaders, Apple Music artwork mirroring, and optional live YouTube backdrop mirroring via a companion Chrome extension.

## Workspace Layout

| Path | Description |
| --- | --- |
| `crates/afterlife-core` | Pure domain engine (`District`, `Philosophy`, `ProcedureSpec`, `Job`, leases, fencing, feedback) |
| `crates/afterlife-protocol` | Shared DTOs, config schemas, and storage/telemetry/event port traits |
| `crates/afterlife-storage` | Local SQLite WAL, filesystem blob store, and JSONL telemetry adapters |
| `crates/afterlife-daemon` | `afterlifed` HTTP/SSE/MCP server + OAuth 2.1 broker |
| `crates/afterlife-cli` | `afterlife` CLI and live `afterlife toolmap` contract |
| `crates/afterlife-design` | Closed design tokens (`SpacePx`, `FontPx`, `FadePx`, glass presets) and UI primitives |
| `crates/afterlife-ui` | Native macOS desktop application built on GPUI + Metal |
| `tools/chrome-youtube-mirror` | Chrome MV3 extension for mirroring playing YouTube frames to the local desktop backdrop |
| `vendor/gpui` | Modified vendored copy of Zed's GPUI 0.2.2 (Apache-2.0; see [`NOTICE`](NOTICE)) |
| `infra/` | Dockerfile and Terraform templates for AWS ECS/RDS/S3 deployment |

## Prerequisites

- **Rust** (stable toolchain, Edition 2024)
- **macOS** (Apple Silicon / Metal) for building and running the desktop UI (`afterlife-ui`)
- Linux / macOS supported for headless crates (`afterlife-core`, `afterlife-protocol`, `afterlife-storage`, `afterlife-daemon`, `afterlife-cli`)

## Quick Start

### Run the Desktop App (macOS)

```bash
cargo run --release -p afterlife-ui
```

### Build & Sign `dist/Afterlife.app` (macOS)

```bash
./scripts/bundle-macos.sh
open dist/Afterlife.app
```

### Run the Headless Daemon & CLI

```bash
cargo run -p afterlife-daemon -- --config config/local.toml
cargo run -p afterlife-cli -- toolmap
```

### Companion Chrome Extension (Optional)

Load `tools/chrome-youtube-mirror` as an unpacked extension in `chrome://extensions` to stream playing YouTube video frames over loopback (`127.0.0.1:47374`) to the desktop backdrop.

## Testing & Verification

```bash
# Workspace tests (excluding vendored gpui)
cargo test --workspace --exclude gpui

# Vendored GPUI unit & Metal regression tests (macOS)
cargo test -p gpui --features runtime_shaders --lib

# Black-box Human x MCP E2E suite
./scripts/e2e.sh
```

## License

Afterlife is licensed under the **GNU Affero General Public License v3.0 (`AGPL-3.0-only`)**. See [`LICENSE`](LICENSE) for the full text.

- **Third-Party Components:** `vendor/gpui` is a modified copy of Zed's GPUI 0.2.2, licensed under the **Apache License 2.0** (`vendor/gpui/LICENSE-APACHE`). See [`NOTICE`](NOTICE) and [`vendor/gpui/NOTICE`](vendor/gpui/NOTICE) for attribution and the list of modified files.
- **Commercial Licensing:** If you wish to use or distribute Afterlife under terms other than AGPL-3.0, please contact the copyright holder for a commercial license.
- **Contributions:** Issues and bug reports are welcome. Please open an issue to discuss before submitting pull requests.
