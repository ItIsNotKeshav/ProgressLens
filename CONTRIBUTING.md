# Contributing to ProgressLens

Thank you for considering a contribution. This document covers how to get set up,
what to work on, and how to submit changes.

---

## Getting Started

1. Fork the repository and clone your fork
2. Follow the [Installation](README.md#installation) guide to set up your environment
3. Create a feature branch: `git checkout -b feat/your-feature-name`
4. Make your changes with clear, focused commits
5. Open a pull request against the `main` branch

---

## Development Setup

```bash
# Install frontend dependencies
npm install

# Set your Google OAuth credentials
# Edit src-tauri/.cargo/config.toml with your GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET

# Start the Tauri development server
npm run tauri dev
```

---

## Running Tests

```bash
# Rust unit and integration tests
cargo test --manifest-path src-tauri/Cargo.toml

# TypeScript type-check
npx tsc --noEmit
```

---

## What to Work On

Check the [Roadmap](README.md#roadmap) in the README for planned features.
Open issues are also a good starting point for smaller improvements.

For larger changes, please open an issue first to discuss the approach before
investing significant time in implementation.

---

## Code Style

- **Rust** — follow `rustfmt` defaults (`cargo fmt`)
- **TypeScript** — follow the existing ESLint configuration
- **SQL** — uppercase keywords, lowercase identifiers, explain non-obvious queries
- **Commits** — use conventional commit prefixes: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`

---

## Reporting Bugs

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md). Include:
- ProgressLens version
- OS and architecture
- Steps to reproduce
- Expected vs actual behaviour
- Relevant log output (from the Tauri dev console)

---

## Security Issues

Do **not** open a public issue for security vulnerabilities. Email the maintainer
directly or use GitHub's private vulnerability reporting feature.

See [docs/security.md](docs/security.md) for the full security model.

---

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE) that covers this project.
