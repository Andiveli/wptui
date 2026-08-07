# Contributing to wptui

Thanks for wanting to help! `wptui` is an experimental, keyboard-driven WhatsApp client for the
terminal. All kinds of contributions are welcome: bug reports, feature requests, documentation,
and code.

> [!NOTE]
> `wptui` is an unofficial client that operates in a WhatsApp terms-of-service gray area. Feature
> requests that depend on unsupported WhatsApp behavior (for example, voice/video calling) may be
> declined as out of scope.

## Getting started

The quickest path to a first contribution:

1. **Find something to work on.** Issues labeled [`good first issue`](https://github.com/Andiveli/wptui/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) are a great starting point. Comment on the issue to say you're taking it.
2. **Fork and clone** the repository.
3. **Create a branch** from `main` with a descriptive name, e.g. `feat/chat-search` or `fix/clipboard-wayland`.
4. **Make your change** following the [style](#code-style) and [testing](#testing) guidance below.
5. **Open a pull request** using the [PR template](.github/pull_request_template.md).

If you're not sure whether an idea fits the project, open an issue first and ask before writing a
lot of code.

## Development setup

### Build requirements

- A recent stable Rust toolchain (the repository pins Rust 1.96.1 via `rust-toolchain.toml`)
- Go with CGO enabled
- A C compiler supported by CGO
- System libraries:

  | OS | Packages |
  |---|---|
  | Debian / Ubuntu | `libwayland-dev libchafa-dev libglib2.0-dev` |
  | macOS | `chafa` (via Homebrew) |

### Build and run

```bash
cargo build --all-targets
```

Run the client with:

```bash
cargo run
```

The Rust build automatically compiles the bundled Go bridge. See the
[README](README.md#build-requirements) for runtime dependencies.

## Testing

CI runs these checks on every push and pull request, so run them locally before opening a PR:

| Check | Command |
|---|---|
| Rust test suite (single-threaded, on purpose) | `cargo test -- --test-threads=1` |
| Go test suite | `cd whatsrust/lib && go test ./...` |
| Go vet | `cd whatsrust/lib && go vet ./...` |
| Rust formatting | `cargo fmt --check` |

Notes:

- The Rust tests **must** run with `--test-threads=1`; they are not safe to run in parallel.
- CI does not run `clippy`, but keeping `cargo clippy` clean is a good idea.

## Code style

- **Rust**: format with `rustfmt` (enforced by CI). Follow the surrounding code's conventions.
- **Go**: format with `gofmt` and keep `go vet` clean.
- **Tests**: add or update tests alongside behavior changes. The Rust integration tests live in
  `tests/`; the Go bridge tests live in `whatsrust/lib/`.

### Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <short description>
```

Common types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `build`, `ci`, `style`, `perf`.

Keep commits atomic — one logical change per commit. Never add AI attribution footers
("Co-Authored-By" or similar) to commits.

## Pull requests

- **Keep PRs small and focused.** Prefer several small PRs over one large one. If a change is
  bigger than a few hundred lines, consider splitting it.
- **Title** the PR with a Conventional Commit message, e.g. `feat(ui): add chat search`.
- **Describe** what and why using the [PR template](.github/pull_request_template.md).
- **Link** the issue your PR closes with `Closes #N`.
- **Make sure all checks pass**: formatting, build, and both test suites.

## Issues

- Check for [existing issues](https://github.com/Andiveli/wptui/issues) before opening a new one
  (open and closed) to avoid duplicates.
- Use the templates in `.github/ISSUE_TEMPLATE/` — a bug report or feature request form will be
  offered automatically.
- For bugs, include environment details: OS, display server (Wayland/X11), terminal emulator, and
  `wp-tui` version or commit. Paste error messages verbatim. If you can, reproduce with the
  log view open (`Ctrl+Shift+L`).
- For feature requests, explain the problem you're trying to solve and the proposed solution.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE),
the same license as the project.
