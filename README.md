# wptui

A keyboard-driven WhatsApp client for the terminal.

`wptui` pairs a Rust [Ratatui](https://github.com/ratatui/ratatui) interface with the Go
[whatsmeow](https://github.com/tulir/whatsmeow) library, giving you a fast, Vim-flavored chat
experience without leaving your terminal.

## Demo

<div align="center">

https://github.com/user-attachments/assets/f08d20f9-6051-45e2-a1d2-b0a512242e9c

</div>

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
- Play animated WebP stickers while visible in terminals with image support (up to four animations at a time, prioritizing visible stickers as you scroll); long animations are sampled to preserve motion and their normalized frame timing within bounded memory, while excess simultaneous or unsupported stickers retain a still preview
- Display contact avatars and desktop notifications
- Contextual, Vim-inspired keybindings
- Andiveli welcome mark in the chat panel before any conversation is opened

The Chats list has **All**, **Unread**, **Groups**, and **Archived** views. All,
Unread, and Groups exclude archived chats; Archived shows only archived chats.
Filter badges count chats with unread messages (each chat once), not individual
messages. All, Unread, and Groups count unarchived chats; Archived counts archived
chats. Zero counts are hidden.
Archive status comes from the linked device's WhatsApp settings. Search narrows
the selected view. Switching views does not change messages or read status;
a community row contains only groups in that view. Archive changes received
from WhatsApp refresh the current view without switching filters. A yellow `@`
and yellow **Archived** label signal an unread mention or reply to your message
in an archived group; ordinary archived chats and other group messages do not
trigger it. Reading that group clears the alert.

## Data

Your WhatsApp data — database, media, and session — lives in `~/.local/share/wptui/` on your
machine. It is not uploaded anywhere; the client only exchanges data with the WhatsApp servers
when it links or syncs, like any official client.

Preferences live in your OS configuration directory (`~/.config/wptui/settings.conf` on Linux).
An existing `settings.conf` in the data directory is copied there on first use if no config
file exists; the legacy file is left untouched. Set `composer_direction=rtl` or
`composer_direction=auto` to choose the composer direction.

Eligible desktop notifications play the bundled `assets/sounds/attention.wav` by default,
even when the notification server does not support sound hints. To use your OS/theme sound
instead, set `notification_sound=system` in `settings.conf` (this requests a system sound hint;
it depends on your notification server). To play your own sound directly, set
`notification_sound=/absolute/path/to/alert.ogg` (WAV, MP3 and OGG/Vorbis are supported).
Absent or invalid settings use the bundled sound. Direct playback does not request a second
system sound; an unreadable file or missing audio device is logged without suppressing the
desktop notification. Messages timestamped before this app session started still show desktop
notices, but request neither bundled/custom audio nor a system sound hint. Messages from the
startup second onward remain audible, even while synchronization progress is visible. If the
startup clock is unavailable, audio remains enabled for eligible notices. Muted chats, open
chats and explicit sync traffic remain silent.

Mouse capture is enabled by default during the pointer-support preview. Add `mouse=disable` to
that config file and restart to restore terminal-native selection, or use
`mouse=enable` to explicitly enable terminal-wide capture. When capture is on, Shift+drag may
allow native text selection depending on your terminal; it cannot be enabled for individual panes.
In this preview, the wheel scrolls painted lists. Clicking a section enters its list, clicking
a contact opens it, and clicking an ordinary text message selects it and opens its Enter actions.
Clicking a painted URL opens that link, while clicking a file's painted media/path opens the
image/video viewer or uses the existing downloaded document/audio action. Empty file-row space
only selects. Mouse behavior still needs a fresh live-terminal check.

## Requirements

### Runtime dependencies

The prebuilt binary links against a few system libraries. On a desktop
installation most of them are already present; the one you usually need to
install explicitly is `libchafa`, which is used to render images in the
terminal.

- **Debian / Ubuntu**
  ```
  sudo apt install libchafa0t64 libglib2.0-0 libwayland-client0 libasound2
  ```
- **Fedora**
  ```
  sudo dnf install chafa glib2 wayland alsa-lib
  ```
- **Arch Linux**
  ```
  sudo pacman -S chafa glib2 wayland alsa-lib
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
  `libglib2.0-dev`, `libasound2-dev`; on macOS, `chafa` from Homebrew
- Optional: `mpv` for opening media externally

## Installation

### Prebuilt binary

Prebuilt binaries for Linux (x86_64) and macOS (Apple Silicon) are
attached to each GitHub release. Install the one for your system with:

```bash
curl -sSL https://raw.githubusercontent.com/Andiveli/wptui/main/install.sh | bash
```

The script downloads the matching binary from the latest release into
`~/.local/bin` (override with `PREFIX`, e.g. `PREFIX=/opt/wptui ./install.sh`).
New release archives include the bundled tone's MIT notice; the installer keeps
it next to the binary as `wp-tui-THIRD_PARTY_NOTICES.md`.
Make sure the runtime libraries listed in [Requirements](#requirements) are
present first.

### Nix (Linux)

The flake declares `wp-tui` for `x86_64-linux` and `aarch64-linux`; CI builds
only the x86_64 Linux package. From a checkout, build the package or run it
from the development shell:

```bash
nix build .#default
./result/bin/wp-tui --version
nix develop --command wp-tui --version
```

Run `nix develop` without `--command` to enter a shell with `wp-tui` on `PATH`.
When using the repository as a NixOS flake input, select
`inputs.wptui.packages.x86_64-linux.default` on x86_64 Linux (or
`inputs.wptui.packages.aarch64-linux.default` on ARM Linux).

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
wp-tui
```

Or request a pairing code using an international-format phone number:

```bash
wp-tui --phone +1234567890
```

Check for and install the latest stable release when using a standalone binary:

```bash
wp-tui update
```

The updater accepts older binary-only archives and newer archives containing the
bundled tone's MIT notices. It keeps `wp-tui-THIRD_PARTY_NOTICES.md` beside the
updated executable.

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
| Focus previous / next pane | `h` / `l` or `←` / `→` |
| Select previous / next item | `k` / `j` or `↑` / `↓` |
| Jump to first / last item | `g g` / `G` |
| Move half a page | `Ctrl+U` / `Ctrl+D` |
| Search chats | `/` |
| Cycle chat filters (All, Unread, Groups, Archived) | `Tab` in the Chats list |

### Messages

| Action | Key |
|---|---|
| Start composing | `i` |
| Start optional Vim composer (Chats/Communities conversation only) | `Space`, then `v` |
| Copy selected text | `y` |
| React | `r` |
| Share | `s` |
| Reply | `R` |
| Edit your text message | `e` |
| Delete your text message | `d` |
| Delete one selected, published own status (text, image, or video) | `d` in Status navigation |
| Open downloaded media | `x` |
| View selected image or video | `v` |
| Open message actions | `Enter` |

Use `j`/`k` or the arrow keys inside menus, then press `Enter` to confirm. Available actions depend
on the selected message. In Status, press `Enter` on your own contact to open the Create status
pane in navigation. Select one published update with `j`/`k` or `↓`/`↑`, then press `d` to revoke it; `i`
starts writing and `Esc` returns to the contact list. `Space`, `a`, `c` still opens Create status
directly in authoring mode; `Esc` there enters navigation. Pending drafts and other contacts'
statuses cannot be deleted this way. A failed revoke leaves the item visible; a successful send
removes the local row, but does not itself prove removal from every recipient's device.

### Composer and media

| Context | Action | Key |
|---|---|---|
| Composer | Send | `Enter` |
| Composer | Paste text, files, or an image | `Ctrl+V` |
| Vim composer Normal | Move cursor | `h`/`j`/`k`/`l` |
| Vim composer Normal | Next / previous word start | `w` / `b` |
| Vim composer Normal | Next / previous word end | `e` / `ge` |
| Vim composer Normal | Next / previous WORD start | `W` / `B` |
| Vim composer Normal | Next / previous WORD end | `E` / `gE` |
| Vim composer Normal | Yank to system clipboard (word/WORD motion) | `yw`/`yb`/`ye`/`yge`/`yW`/`yB`/`yE`/`ygE` |
| Vim composer Normal | Yank current line (including newline) | `yy` |
| Vim composer Normal | Paste system clipboard text at cursor | `p` |
| Vim composer Normal | Insert before / append after current grapheme | `i` / `a` |
| Vim composer Normal | Return to chat (draft kept) | `Esc` |
| Vim composer Insert | Return to Normal | `Esc` |
| Reaction picker | Move / confirm | `j`/`k` or Up/Down arrows / `Enter` |
| Media viewer | Previous / next | `h`/`l` or arrows |

The optional Vim composer starts in Normal mode; its border title shows Normal or Insert.
`i` enters Insert at the current cursor; `a` advances past the current grapheme within the
same line before entering Insert (and stays put at the end of a line or an empty draft).
Ordinary `i` composition remains unchanged. These motions move only the cursor: a word
separates letters/digits/underscores from punctuation, while a WORD is any non-whitespace
run. In Normal mode, `y` waits for a supported motion or another `y`; `Esc` cancels it.
Yanks copy to the system clipboard, without changing the draft or cursor. In Normal mode,
`p` inserts the clipboard's text at the cursor, even after `yy`; it does not use a Vim register
or Vim's after-cursor/linewise put positioning. `p` does not attach images or files; use
`Ctrl+V` while editing to paste those. There are no registers or counts, and this is not full
Vim emulation. Search, substitution, and deletion are not available here. Files pasted with
`Ctrl+V` are queued as attachments; text becomes the caption of the first attachment.

## Development

### Runtime performance diagnostics

The opt-in profiler is disabled unless `WPTUI_PERF=1`. Build and run the
release binary with the exact command `WPTUI_PERF=1 target/release/wp-tui`,
reproduce the CPU spike or interaction, and exit normally. It writes one
bounded report to `~/.cache/wptui/perf-report.txt`.

To build and test without using local Cargo resources, see [cloud-built preview binaries](docs/preview-builds.md). Push a `preview/**` branch based on `main` when you want a downloadable Linux binary without creating a release.

For contributors who prefer local checks, run the Rust test suite:

```bash
cargo test -- --test-threads=1
```

Run the Go test suite:

```bash
cd whatsrust/lib
go test ./...
```

Continuous integration runs format checks, Rust and Go builds/tests, and installer archive tests on both Linux and macOS for pushes to `main` and pull requests. Nix build and dev-shell checks run on Linux. For on-demand feature binaries, start a `preview/**` branch from `main`; its pushes run preview checks and upload a Linux binary. To build the integrated `main` on demand, manually dispatch `release.yml` on `main`: it uploads artifacts without publishing a release. Tagged releases remain a separate decision.

## Disclaimer

`wptui` is an unofficial, experimental client. It is not affiliated with, endorsed by, or associated
with WhatsApp or Meta Platforms, Inc. Unofficial clients operate in a terms-of-service gray area;
use at your own risk, as an account restriction is possible.

## License

MIT © 2026 andiveli. See [LICENSE](LICENSE).

Third-party components are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
