# wptui

A keyboard-driven WhatsApp client for the terminal.

`wptui` pairs a Rust [Ratatui](https://github.com/ratatui/ratatui) interface with the Go
[whatsmeow](https://github.com/tulir/whatsmeow) library, giving you a fast, Vim-flavored chat
experience without leaving your terminal.

## Demo

<!-- Drop the demo video at assets/demo.mp4 (or update the src below). -->
<video src="assets/demo.mp4" controls></video>

> [!WARNING]
> This project is under active development. Features and behavior may change between versions.

## Features

- Link a WhatsApp account with a QR code or a phone pairing code
- Browse and search chats from the terminal
- Send, reply to, edit, delete, copy, and react to messages
- Inspect reactions and quoted messages
- Paste text, images, and files from the clipboard
- Send attachments with captions
- Download and preview media, or open it in an external player
- Display contact avatars and desktop notifications
- Contextual, Vim-inspired keybindings
- Andiveli welcome mark in the chat panel before any conversation is opened

## Data

Your WhatsApp data — database, media, and session — lives in `~/.local/share/wptui/` on your
machine. It is not uploaded anywhere; the client only exchanges data with the WhatsApp servers
when it links or syncs, like any official client.

## Requirements

### Runtime dependencies

The prebuilt binary links against a few system libraries. On a desktop
installation most of them are already present; the one you usually need to
install explicitly is `libchafa`, which is used to render images in the
terminal.

- **Debian / Ubuntu**
  ```
  sudo apt install libchafa0t64 libglib2.0-0 libwayland-client0
  ```
- **Fedora**
  ```
  sudo dnf install chafa glib2 wayland
  ```
- **Arch Linux**
  ```
  sudo pacman -S chafa glib2 wayland
  ```
- **macOS**
  ```
  brew install chafa
  ```
  macOS uses the native file dialogs, so no Wayland libraries are needed.

### Build requirements

- Linux or macOS
- A recent stable Rust toolchain with Cargo (the repository pins Rust 1.96.1
  via `rust-toolchain.toml`)
- Go with CGO enabled
- A C compiler supported by CGO
- On Linux, the development packages `libwayland-dev`, `libchafa-dev` and
  `libglib2.0-dev`; on macOS, `chafa` from Homebrew
- Optional: `mpv` for opening media externally

## Installation

### Prebuilt binary

Prebuilt binaries for Linux (x86_64) and macOS (Apple Silicon and Intel) are
attached to each GitHub release. Install the one for your system with:

```bash
curl -sSL https://raw.githubusercontent.com/Andiveli/wptui/main/install.sh | bash
```

The script downloads the matching binary from the latest release into
`~/.local/bin` (override with `PREFIX`, e.g. `PREFIX=/opt/wptui ./install.sh`).
Make sure the runtime libraries listed in [Requirements](#requirements) are
present first.

### Build from source

```bash
git clone https://github.com/Andiveli/wptui.git
cd wptui
cargo install --path .
```

The Rust build automatically compiles the bundled Go bridge. See
[Requirements](#requirements) for the build-time libraries.

## Usage

Start the client and scan the QR code displayed in the terminal:

```bash
wptui
```

Or request a pairing code using an international-format phone number:

```bash
wptui --phone +1234567890
```

When running through Cargo, separate application arguments with `--`:

```bash
cargo run -- --phone +1234567890
```

### Diagnostics

To trace edited/deleted message processing, run with:

```bash
WPTUI_MESSAGE_ACTION_DEBUG=1 cargo run
```

After quitting, a bounded, redacted trace is printed in the normal terminal.

## Keybindings

Keybindings are contextual. Press `Esc` to close the active menu, picker, search, viewer, reply,
or editor state.

### Global and navigation

| Action | Key |
|---|---|
| Quit | `Ctrl+Q` |
| Toggle logs | `Ctrl+Shift+L` |
| Toggle section rail | `Space`, then `1` |
| Toggle chat list | `Space`, then `2` |
| Focus previous / next pane | `h` / `l` |
| Select previous / next item | `k` / `j` |
| Jump to first / last item | `g g` / `G` |
| Move half a page | `Ctrl+U` / `Ctrl+D` |
| Search chats | `/` |

### Messages

| Action | Key |
|---|---|
| Start composing | `i` |
| Copy selected text | `y` |
| React | `r` |
| Share | `s` |
| Reply | `R` |
| Edit your text message | `e` |
| Delete your text message | `d` |
| Open downloaded media | `x` |
| View selected image or video | `v` |
| Open message actions | `Enter` |

Use `j`/`k` or the arrow keys inside menus, then press `Enter` to confirm. Available actions depend
on the selected message.

### Composer and media

| Context | Action | Key |
|---|---|---|
| Composer | Send | `Enter` |
| Composer | Paste text, files, or an image | `Ctrl+V` |
| Reaction picker | Move / confirm | `h`/`l` or arrows / `Enter` |
| Media viewer | Previous / next | `h`/`l` or arrows |

Pasted files are queued as attachments. When text is included, it becomes the caption of the first
attachment.

## Development

Run the Rust test suite:

```bash
cargo test -- --test-threads=1
```

Run the Go test suite:

```bash
cd whatsrust/lib
go test ./...
```

Continuous integration runs format checks, builds, and both test suites on every push and pull
request.

## Disclaimer

`wptui` is an unofficial, experimental client. It is not affiliated with, endorsed by, or associated
with WhatsApp or Meta Platforms, Inc. Unofficial clients operate in a terms-of-service gray area;
use at your own risk, as an account restriction is possible.

## License

MIT © 2026 andiveli. See [LICENSE](LICENSE).

Third-party components are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
