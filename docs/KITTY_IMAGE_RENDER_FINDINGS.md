# Kitty protocol image render bug – findings (written by AI)

## Symptom

In the **message list**, image thumbnails sometimes fail to render (or render broken) with the **Kitty** graphics protocol when the **first** time the image is drawn it is a **partial render** (message is clipped by the list viewport). After a resize they work. The issue does **not** occur with **Halfblocks**.

## Where it happens

The bug is in the **message list** thumbnail rendering, not in MessageView.

The list has two render paths:

1. **Full render**: message is fully inside the visible list area → we call `render_message(frame.buffer_mut(), ...)` and draw directly into the frame.
2. **Partial render**: message is clipped (top or bottom) → we allocate a **temp buffer** for the full message, call `render_message(&mut buf, ...)` to draw the whole message into that buffer, then **copy only the visible slice** from the buffer to the frame.

When the **first** time an image is ever drawn we take the **partial** path:

- The image is encoded and drawn for the full thumbnail `media_area` (e.g. 36×12) in the **temp** buffer.
- Only the **visible** rows of that buffer are copied to the frame (e.g. 5 rows instead of 12).
- So the frame ends up with only a **partial** set of Kitty placeholder rows for that image.
- Kitty’s stateful protocol then sees an image transmitted for 12 rows but only a subset of placeholder rows on screen; the first render is “partial” and the protocol state (or terminal) can get out of sync. Resize forces a full redraw and fixes it.
- Halfblocks don’t use that stateful path, so they are unaffected.

## Fix (implemented)

When we copy the visible slice from the temp buffer to the frame in the **partial** path, we detect when the visible slice does **not** include the first row of the image (i.e. the message is scrolled so we see the middle or bottom of the image first). In that case Kitty never receives the image **transmit** (the graphics data), because that is only in the first row’s cell in the buffer. We therefore **inject** the transmit into the first visible row’s left cell: we take the prefix of the first image row’s symbol up to `\x1b[s` (start of the placeholder sequence) and prepend it to the first visible row’s cell symbol before copying to the frame. So the first cell we send for that image contains both the transmit and the correct row’s placeholders; Kitty gets the image data and the visible slice, and the image displays correctly even when the first render is partial. No “fully rendered” tracking or placeholder is needed; partial rendering works on first show.

## References

- ratatui-image Kitty: one cell per row, save/restore cursor, placeholders; state is tied to the full rect.
- Message list: `message_list.rs` – `too_low || too_high` branch (temp buffer + copy) vs direct `render_message` into frame.
