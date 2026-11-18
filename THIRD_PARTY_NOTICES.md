# Third-Party Notices

This project depends on third-party software. Notices for components with
distinct license obligations are listed here. The full license texts of Go
modules are distributed in the Go module cache; the full texts of Rust crate
licenses are distributed by the crates themselves and via
`cargo about`-style metadata where applicable.

## go.mau.fi/whatsmeow

- Component: `whatsmeow` — WhatsApp Web protocol client library
- Author: Tulir Asokan (https://github.com/tulir/whatsmeow)
- License: Mozilla Public License 2.0 (MPL-2.0)
- Source: https://github.com/tulir/whatsmeow

The MPL-2.0 full text is available at https://www.mozilla.org/MPL/2.0/ and in
the `LICENSE` file of the `go.mau.fi/whatsmeow` module in the Go module cache.

## Symphonia

- Component: `symphonia` and audio codec crates — audio decoding for custom notification sounds
- License: Mozilla Public License 2.0 (MPL-2.0)
- Source: https://github.com/pdeljanov/Symphonia

The MPL-2.0 full text is available at https://www.mozilla.org/MPL/2.0/ and
in the corresponding crate source distributions.

## Gentle AI attention tone

- Component: `assets/sounds/attention.wav` — bundled notification tone
- Author: Copyright (c) 2026 Gentle AI contributors
- License: MIT; original asset description and reproduction recipe: `assets/sounds/LICENSE.md`

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Built-in theme palette reference

The UI/base color values in `src/ui/theme.rs` were adapted from the following
modules in the locally inspected `@shikijs/themes` 4.2.0 distribution. No syntax
highlighting rules, editor theme files, or runtime dependency are bundled.

| wptui setting | Reference module / displayed variant |
|---|---|
| `kanagawa` | `kanagawa-wave.mjs` / Kanagawa Wave |
| `one-dark` | `one-dark-pro.mjs` / One Dark Pro |
| `catppuccin` | `catppuccin-mocha.mjs` / Catppuccin Mocha |
| `tokyo-night` | `tokyo-night.mjs` / Tokyo Night |
| `gruvbox` | `gruvbox-dark-medium.mjs` / Gruvbox Dark Medium |

Distribution source: https://github.com/shikijs/shiki/tree/main/packages/themes

The inspected package's `LICENSE` is MIT and states:

- Copyright (c) 2021 Pine Wu
- Copyright (c) 2023 Anthony Fu <https://github.com/antfu>

The MIT permission grant, inclusion condition and warranty disclaimer reproduced
above also accompany this reference distribution. These distribution credits do
not replace the following upstream palette-family credits. Primary sources
consulted for attribution are:

- Kanagawa: Copyright (c) 2021 Tommaso Laurenzi; MIT License.
  <https://github.com/rebelot/kanagawa.nvim/blob/master/LICENSE>
- One Dark Pro: Copyright (c) 2013-2022 Binaryify; The MIT License (MIT).
  <https://github.com/Binaryify/OneDark-Pro/blob/master/LICENSE.txt>
- Catppuccin: Copyright (c) 2021 Catppuccin; MIT License.
  <https://github.com/catppuccin/catppuccin/blob/main/LICENSE>
- Tokyo Night: Copyright (c) 2018-present Enkia; The MIT License (MIT).
  <https://github.com/tokyo-night/tokyo-night-vscode-theme/blob/master/LICENSE.txt>
- Gruvbox: author morhetz; the upstream README declares MIT/X11.
  <https://github.com/morhetz/gruvbox/blob/master/README.md>
  <https://github.com/morhetz/gruvbox/blob/master/colors/gruvbox.vim>
  No copyright year is inferred from the inspected header.

The MIT permission grant, inclusion condition and warranty disclaimer reproduced
above are retained with these MIT-licensed palette references. These links
identify upstream families, not a verified file-by-file history of Shiki's
converted variants. Only color values are adapted; upstream editor or plugin
implementation code is not included.

wptui's semantic-role choices are adaptations: explicit opaque surfaces, brighter
secondary/border colors and light selected text are chosen for terminal
readability. Theme names identify the palette families, not endorsement.

## Other dependencies

Remaining Go and Rust dependencies are consumed under their respective
licenses (commonly MIT and/or Apache-2.0). This project does not modify or
redistribute their source files.
