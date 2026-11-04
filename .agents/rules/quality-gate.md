# Quality Gate & Verification Protocol

## Mandatory Execution Pipeline

Before completing any code modification, refactor, or committing changes, execute:

1. **Formatting**:

   ```bash
   cargo fmt --all
   ```

   Ensure `cargo fmt --all -- --check` has zero diffs.

2. **Dependency Unification**:

   ```bash
   cargo hakari generate
   ```

   Required whenever `Cargo.toml` or dependencies are modified. Stage all changes to `workspace-hack/Cargo.toml` and `Cargo.lock`.

3. **Linter (Clippy)**:

   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

   Must pass with zero errors and zero warnings.

4. **Test Suite**:

   ```bash
   cargo test --all
   ```

   All unit, doc, and integration tests must pass.

5. **Hook Compliance**:

   Do not bypass `.githooks/pre-commit` with `--no-verify`. Resolve all check failures directly.
