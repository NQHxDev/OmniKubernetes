# Contributing to OmniKubernetes

Thank you for considering contributing to OmniKubernetes! We welcome contributions, bug reports, feature requests, and documentation improvements.

## Code of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Getting Started

1. **Fork** the repository on GitHub.
2. **Clone** your fork locally:
    ```bash
    git clone https://github.com/<your-username>/OmniKubernetes.git
    cd OmniKubernetes
    ```
3. **Create a branch** for your feature or bug fix:
    ```bash
    git checkout -b feature/your-feature-name
    # or
    git checkout -b fix/your-bugfix-name
    ```

## Development Workflow

### Prerequisites

- **Rust toolchain:** Latest stable version (Rust 2024 edition compatible).
- **Components:** `rustfmt` and `clippy`.
    ```bash
    rustup component add rustfmt clippy
    ```

### Quality Checks

Before submitting a pull request, ensure your code passes all checks:

1. **Format Check:**
    ```bash
    cargo fmt --check
    ```
2. **Linter Check:**
    ```bash
    cargo clippy --all-targets --all-features -- -D warnings
    ```
3. **Run Tests:**
    ```bash
    cargo test --all-targets
    ```

### Commit Message Guidelines

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

- `feat: add new gateway routing feature`
- `fix: resolve auth token expiration bug`
- `docs: update deployment instructions in README`
- `refactor: simplify request handler logic`
- `test: add unit tests for token validation`
- `ci: update GitHub Actions workflows`

## Submitting a Pull Request

1. Push your branch to your fork on GitHub:
    ```bash
    git push origin feature/your-feature-name
    ```
2. Open a Pull Request against the `develop` (or `main`) branch of `NQHxDev/OmniKubernetes`.
3. Provide a clear description of the problem solved and the changes introduced.
4. Link any related issues (e.g., `Closes #12`).
5. Ensure all CI checks pass.

Thank you for contributing to OmniKubernetes!
