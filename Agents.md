# OmniKubernetes Agent Guidelines

## 1. Project Overview

- **Toolchain:** Rust 2024 edition, Rust 1.85+.
- **Workspace:** Microservices in `services/*`, shared libraries in `libs/*`, dependencies unified in `workspace-hack/`.
- **Patterns:** RFC 9457 Problem Details (`omni-errors`), structured ECS/OTel logging with PII masking (`omni-logging`, `omni-common::middleware`).

## 2. Definition of Done (Task Completion Protocol)

Before marking any task complete or committing code, run and pass:

```bash
# 1. Format code
cargo fmt --all

# 2. Sync dependencies (if Cargo.toml was touched)
cargo hakari generate

# 3. Lint check (zero warnings allowed)
cargo clippy --all-targets --all-features -- -D warnings

# 4. Test suite
cargo test --all
```

- Never bypass `.githooks/pre-commit` using `--no-verify`.
- Rule details: [.agents/rules/quality-gate.md](file://.agents/rules/quality-gate.md)

## 3. Git Conventions

- Follow Conventional Commits: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`.
- Stage only intentional changes; do not commit untracked scratch/debug artifacts.
