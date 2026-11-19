# What's new in wptui 0.4.0

## Chats and messages

- **See who's typing.** The open conversation shows typing activity at the bottom of its message list, including multiple people in a group. The indicator is temporary and isn't saved as a message.
- **Read and write formatted messages.** WhatsApp formatting and enriched Markdown render in messages and attachment captions. The composer previews formatting while you type and reveals the markers where the cursor is. Sending, copying, and searching still use the original text.
- **Read links at a glance.** Bare links display a readable host and path label without changing where they open. When WhatsApp supplies a native preview, the chat can show its title, description, and embedded thumbnail without fetching the website.
- **Search the open conversation.** Press `/` with the message list focused to search loaded text messages, then use `n` and `N` to move between matches. The composer also has an optional Vim mode (`Space → v`) and keeps line breaks when you paste a block of text.

## Photos, videos, and stickers

- Preview queued images and videos before sending. Consecutive photos and videos from the same sender group into a stack; you can still navigate and act on every message individually.
- Animated WebP stickers play while visible on terminals that support images; a still preview is used when animation isn't available.
- Press `s` in the media viewer to choose a folder and filename for a photo or video. Existing files are never overwritten.

## Make it yours

- Open `Space → a → t` to apply Kanagawa, One Dark, Catppuccin, Tokyo Night, or Gruvbox without restarting. Use `theme.json` for a custom palette, or choose Default to keep the terminal's usual appearance.
- Configure notification sounds in `settings.conf`. Toggle notifications and audio with `Space → a → n`.
- Open contact details alongside the chat to see the profile photo, phone number when known, and known shared groups. Search finds saved contacts without chat history, and a verified international number can open a conversation without sending a message.

## News, upgrades, and fixes

- **News wptui** keeps the installed version's notes available offline and shows notes for newer stable releases when online. Where supported, you can choose an update and restart on your terms; restarting is deferred while drafts or pending sends need attention.
- The Chats filter badges count unread chats rather than messages. The contact menu can mark a chat read or unread locally. **Known limitation:** do not rely on the corresponding state syncing to the WhatsApp phone app ([#517](https://github.com/Andiveli/wptui/issues/517)).
- Preferences use the system configuration directory; an existing settings file is preserved during migration. This release also improves startup error reporting for SQLite and updates dependencies and Nix packaging.
