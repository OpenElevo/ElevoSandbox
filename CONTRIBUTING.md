# Contributing to ElevoSandbox

Thank you for your interest in contributing to ElevoSandbox!

## How to Contribute

### Reporting Issues

- Use GitHub Issues to report bugs or request features
- Search existing issues before creating a new one
- Provide detailed information including:
  - Steps to reproduce (for bugs)
  - Expected vs actual behavior
  - Environment details (OS, Docker version, etc.)

### Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test --all`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

### Development Setup

```bash
# Clone the repository
git clone https://github.com/OpenElevo/ElevoSandbox.git
cd ElevoSandbox

# Build the project
cargo build

# Run tests
cargo test --all

# Start the server (development)
cd server && cargo run
```

### Code Style

- Rust: Follow standard Rust formatting (`cargo fmt`)
- Run clippy before submitting: `cargo clippy --all-targets --all-features`
- Write meaningful commit messages
- Add tests for new features

### SDK Contributions

Each SDK has its own directory:

- **Go SDK**: `sdk-go/`
- **Python SDK**: `sdk-python/`
- **TypeScript SDK**: `sdk-typescript/`

Please follow the conventions of each language/ecosystem.

## License

By contributing to ElevoSandbox, you agree that your contributions will be licensed under the MIT License.
