# Contributing to NMCScan

Thank you for your interest in contributing! Here are a few guidelines to help you get started.

## Setting Up for Development

```bash
# Clone the repository
git clone https://github.com/ntech-org/nmcscan.git
cd nmcscan

# Set up environment
cp .env.example .env

# Run tests
cargo test

# Run the scanner in test mode (scans known servers only)
cargo run -- --test-mode
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy -- -D warnings` to catch common issues
- All tests must pass: `cargo test`

## Project Structure

| Directory | Purpose |
|-----------|---------|
| `src/network/` | Protocol implementations (SLP, RakNet, Login) |
| `src/services/` | Background tasks (scheduler, login queue, ASN fetcher) |
| `src/handlers/` | HTTP API endpoints (Axum) |
| `src/models/` | Domain models and SeaORM entities |
| `src/repositories/` | Database access layer |
| `src/utils/` | Utilities (exclude list, query parser) |
| `migration/` | Database schema migrations |
| `dashboard/` | SvelteKit frontend |

## Submitting Changes

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes (`git commit -m 'Add my feature'`)
4. Push to the branch (`git push origin feature/my-feature`)
5. Open a Pull Request

### Pull Request Guidelines

- Describe what the change does and why
- Include tests for new functionality
- Update documentation if behavior changes
- Keep commits focused and atomic

## Reporting Bugs

Please include:
- The version/commit you're running
- Steps to reproduce the issue
- Expected vs actual behavior
- Relevant log output (`RUST_LOG=debug`)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
