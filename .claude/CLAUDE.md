# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# Project Overview

Oxen is a fast, unstructured data version control system written in Rust. It's designed to version large machine learning datasets efficiently and provides both a CLI tool and server implementation.

# Project Organization

Cargo workspace at the repository root, with crates under `crates/`:
- `crates/lib/` - Core shared library (`liboxen`) — all business logic lives here
- `crates/cli/` - CLI binary (`oxen`) — thin wrapper over `liboxen`
- `crates/server/` - HTTP server binary (`oxen-server`) — actix-web, calls `liboxen::repositories::*` directly
- `crates/oxen-py/` - PyO3 Rust source for Python bindings
- `oxen-python/` - Python package source, tests, and `pyproject.toml`

## Architecture

All core functionality is implemented in `liboxen` first, then exposed through CLI or server interfaces. The server never touches a local checkout on disk for API-initiated operations.

### `crates/lib/src/` layers (top → bottom)

```
repositories/          ← Public API: high-level operations (add, commit, push, pull, …)
core/v_latest/         ← Current format implementation; mirrors repositories/ structure
core/v_old/v0_19_0/    ← Legacy format shims for migration
core/db/               ← Raw DB access: RocksDB (key-val, merkle_node) + DuckDB (data_frames)
model/                 ← Pure data structs: Commit, Branch, Entry, MerkleTree, Schema, …
view/                  ← API response shapes (serialised to JSON by the server)
storage/               ← Pluggable backends: local filesystem, S3
error/                 ← OxenError enum (top-level error type for all lib code)
```

**Most new feature work:** implement in `repositories/` (calling into `core/v_latest/`), expose in `cli/` or `server/controllers/`.

### Core data structure: `CommitMerkleTree`

The Merkle tree (`core/v_latest/index/commit_merkle_tree.rs`) is the central data structure for change detection. Nodes are persisted in RocksDB (`core/db/merkle_node/`). Understanding this is essential when working on commit, diff, or push/pull logic.

### Workspaces

A *workspace* is a lightweight staged-change area attached to a remote repo (not a local checkout). Server workspace operations live in `server/controllers/workspaces/` and `core/workspaces/`. They allow clients to stage, inspect, and commit changes without cloning.

## Common Development Commands

**IMPORTANT**: Always run cargo commands from the workspace root; never target individual packages.
```bash
# Good
cargo check --workspace
# Bad
cargo check --package liboxen
```

### Building
```bash
cargo build --workspace
```

### Testing

Use `bin/test-rust` instead of `cargo test` directly:
```bash
bin/test-rust                        # All Rust tests
bin/test-rust test_function_name     # Tests matching a name
bin/test-rust -p                     # All Python tests (builds via maturin)
bin/test-rust -p -k test_init        # Python tests matching test_init
bin/test-rust --install-deps         # Install prerequisites then run
```

If the server is not running on port 3000 and a test fails to connect, start it first:
```bash
ulimit -n 10240 && cargo run -p oxen-server start
```

Debug output for a single test:
```bash
env RUST_LOG=warn,liboxen=debug cargo test -- --nocapture test_name
```

### Code Quality
```bash
cargo fmt --all
cargo clippy --workspace --no-deps -- -D warnings
pre-commit run --all-files           # runs fmt + clippy together
```

### Server Development
```bash
ulimit -n 10240
bacon server                         # live-reload server
```

## Code Organization
- Module exports use a `<module_name>.rs` file at the same level as the `module_name/` directory — not the `mod.rs` pattern.
- Tests go in `repositories/` (high-level), not `core/v_latest/` (implementation detail).

## Error Handling
- Return `Result<T, OxenError>` for anything that can fail. `OxenError` is the unified top-level error type.
- Never use `.unwrap()` or `.expect()` outside of test code.
  - In tests, `.expect("<invariant description>")` is acceptable for fast failure with a clear message.
- Use the most specific error type possible within a module; wrap into `OxenError` at module boundaries via `#[from]` + `Box<>`.
- When adding an `OxenError` variant, update the `hint()` method if a user-facing hint applies.
- Propagate with `?`; never silently swallow errors.

## Making Changes

- **`bin/test-rust`** — always use this script, not `cargo test`, to run Rust tests.
- **`bin/install-prereqs`** — run if dependencies are missing (or pass `--install-deps` to `bin/test-rust`).
- **After any Rust or Python change** — verify with `bin/test-rust` and `bin/test-rust -p`.
- **IO code** — all file-system and network code must be async (`tokio::fs`, `tokio::io`). Use `tokio::task::spawn_blocking` when a dependency lacks async support.
- **`get_staged_db_manager`** — drop the returned `StagedDBManager` as soon as possible (block scope or explicit `drop()`).
- **Inline vs function** — prefer inline code when a function would only be called once and is < 15 lines.
- **Python bindings** — when changing Rust-exposed APIs, check whether `crates/oxen-py/` and `oxen-python/` need updates.
- **Dependencies** — prefer updating to the latest stable version.
- **Comments** — preserve all existing comments; update them if the surrounding code changes.
- **Documentation** — update nearby markdown files and doc comments when changing documented behaviour.
- **This file** — add new standing rules here when instructed to "always do X".

## Testing Rules

Test helpers in `crates/lib/src/test.rs` (choose the minimal one for the scenario):

| Helper | When to use |
|--------|-------------|
| `run_empty_dir_test` | Just need a temp directory |
| `run_empty_local_repo_test` | Need an initialised local repo |
| `run_one_commit_local_repo_test` | Need one committed file, local only |
| `run_one_commit_sync_repo_test` | Need one commit synced to a remote |
| `run_training_data_fully_sync_remote` | Need full dataset pushed to remote |

Async variants end in `_async`. Tests create unique temp directories and clean up automatically.
