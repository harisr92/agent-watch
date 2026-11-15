# AGENTS.md

Guidance for AI coding agents (and humans) working in this repository.
`CLAUDE.md` is a symlink to this file. Edit `AGENTS.md` only.

## Project layout

| Path | Responsibility |
|------|----------------|
| `crates/aw-core` | Pure domain logic: prompt detection (`detector.rs`), output buffers (`buffer.rs`), shared models (`models.rs`). No I/O, no async runtime. |
| `crates/aw-daemon` | HTTP/WebSocket/MCP daemon on `127.0.0.1:45123` (`server.rs`) and agent state (`state.rs`). Integration tests in `tests/api_tests.rs`. |
| `crates/aw-cli` | `aw` binary: PTY wrapper (`pty_runner.rs`) and subcommands (`main.rs`). |
| `macOS/AgentWatch` | Swift/SwiftUI menu bar app (`Models/`, `Services/`, `Views/`). |
| `scripts/` | Packaging, install, LaunchAgent plists, mock agents for manual testing. |

Dependency direction: `aw-cli` / `aw-daemon` → `aw-core`. `aw-core` never depends on the other crates.

## Required checks (run before every commit)

These checks mirror `.github/workflows/ci.yml`. CI fails on any warning.

### Rust

```bash
cargo fmt --all                                        # format (CI runs with -- --check)
cargo clippy --workspace --all-targets -- -D warnings  # lint, warnings are errors
cargo test --workspace                                 # unit + integration tests
```

Also keep `cargo check --workspace --all-targets` free of `unused dependency` warnings.
Remove a crate from `Cargo.toml` (and from `[workspace.dependencies]` if nothing else uses it) once it is no longer imported.

### Swift

```bash
cd macOS/AgentWatch
swift build
swift test
```

### Manual end-to-end check

```bash
cargo run -p aw-cli -- daemon &          # start the daemon (aw daemon)
cargo run -p aw-cli -- run python3 scripts/mock_agent.py
```

## Testing rules

- Every bug fix comes with a regression test. Every new feature comes with tests for its happy path and its main edge cases.
- Put unit tests next to the code (`#[cfg(test)] mod tests`). Put HTTP/WebSocket/MCP behaviour tests in `crates/aw-daemon/tests/`.
- Prompt detection changes in `aw-core/src/detector.rs` need table-style tests covering both positive matches and near-miss negatives.
- Tests must be deterministic: no real network beyond loopback, no sleeps for synchronisation, no dependence on the user's home directory.
- Don't delete or weaken an existing test to make a change pass.

## Formatting & lint rules

- Use `rustfmt` defaults. Don't hand-align code or add `#[rustfmt::skip]` without a reason in a comment.
- Fix clippy warnings at the source. `#[allow(clippy::...)]` needs a one-line justification comment and the narrowest possible scope.
- No unused imports, variables, or dependencies. Prefix a variable with `_` only when it is intentionally unused, such as a guard.
- Swift: follow the existing style in `Sources/AgentWatch`. Keep views free of networking code.

## Design principles (SOLID)

**S: Single responsibility.** Each module, type, and function has one reason to change.
- Detection heuristics live in `aw-core`. Transport lives in `aw-daemon`. Process/PTY handling lives in `aw-cli`. Don't mix them.
- In Swift, `Views/` render state, `Services/` talk to the daemon or OS, and `Models/` hold data.
- Split a function when it grows past roughly 50 lines or needs a comment to explain its sections.

**O: Open/closed.** Extend behaviour without editing stable code.
- Add new prompt kinds by adding a detector rule or variant, not by growing `if/else` chains at call sites.
- Use `enum` + `match` for closed sets (prompt kinds, statuses). Use traits for open sets (agent sources, notifiers).

**L: Liskov substitution.** Any implementation of a trait or protocol must honour its documented contract: same error semantics, no surprise panics, no stricter preconditions. Test fakes must behave like the real thing.

**I: Interface segregation.** Prefer small, focused traits and protocols over one large one. A consumer should depend only on the methods it calls. For example, a type that only reads state shouldn't need a write-capable handle.

**D: Dependency inversion.** High-level logic depends on abstractions, not concrete I/O.
- Inject clocks, stores, PTYs, and HTTP clients through constructors or trait parameters so they can be faked in tests.
- `aw-core` stays pure and synchronous, and is easy to test without a runtime.
- Swift services are passed into views, not created inside them.

Pragmatism beats ceremony: don't add a trait with a single implementation unless a test needs to substitute it.

## Error handling

- Library code (`aw-core`, `aw-daemon` lib) returns typed errors or `Result`, and never calls `unwrap()`/`expect()` on fallible runtime input.
- Binaries may use `anyhow` at the top level for context-rich errors.
- Log with `tracing`, not `println!`, in the daemon.

## Commits

- Keep changes small and focused. Unrelated refactors go in a separate commit.
- All required checks above must pass locally before you commit.
