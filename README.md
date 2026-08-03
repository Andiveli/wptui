# wptui

A keyboard-driven WhatsApp client for the terminal.

`wptui` pairs a Rust [Ratatui](https://github.com/ratatui/ratatui) interface with the Go
[whatsmeow](https://github.com/tulir/whatsmeow) library, giving you a fast, Vim-flavored chat
experience without leaving your terminal.

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

## Demo

<!-- Drop the demo video at assets/demo.mp4 (or update the src below). -->
<video src="assets/demo.mp4" controls></video>

## Data

Your WhatsApp data — database, media, and session — lives in `~/.local/share/wptui/` on your
machine. It is not uploaded anywhere; the client only exchanges data with the WhatsApp servers
when it links or syncs, like any official client.

## Requirements

- Linux or another Unix-like operating system
- A recent stable Rust toolchain with Cargo
- Go with CGO enabled
- A C compiler supported by CGO
- Optional: `mpv` for opening media externally

## Installation

Prebuilt Linux binaries are attached to each GitHub release.

To build and install from source:

```bash
git clone https://github.com/Andiveli/wptui.git
cd wptui
cargo install --path .
```

The Rust build automatically compiles the bundled Go bridge.

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
