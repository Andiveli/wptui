use std::sync::Arc;

pub const NATIVE_MATCHED_TEXT_LIMIT: usize = 8_192;
pub const NATIVE_TITLE_LIMIT: usize = 2_048;
pub const NATIVE_DESCRIPTION_LIMIT: usize = 8_192;
pub const NATIVE_THUMBNAIL_LIMIT: usize = 256 * 1_024;

/// WhatsApp-supplied metadata, never a replacement for the source URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLinkPreview {
    pub matched_text: Arc<str>,
    pub title: Arc<str>,
    pub description: Arc<str>,
    pub jpeg_thumbnail: Vec<u8>,
}

fn safe_text(text: &str, limit: usize) -> bool {
    text.len() <= limit
        && !text.chars().any(|character| {
            character.is_control()
                || matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

impl NativeLinkPreview {
    /// Reject unsafe text, retaining legitimate Unicode. Oversized images alone
    /// degrade to text-only; no decoding or remote fallback occurs here.
    pub fn bounded(mut self) -> Option<Self> {
        if self.matched_text.is_empty()
            || !safe_text(&self.matched_text, NATIVE_MATCHED_TEXT_LIMIT)
            || !safe_text(&self.title, NATIVE_TITLE_LIMIT)
            || !safe_text(&self.description, NATIVE_DESCRIPTION_LIMIT)
        {
            return None;
        }
        if self.jpeg_thumbnail.len() > NATIVE_THUMBNAIL_LIMIT {
            self.jpeg_thumbnail = Vec::new();
        }
        Some(self)
    }
}

// These pointers are borrowed from the synchronous Go emitter. Length checks
// happen before constructing slices or allocating owned Rust buffers.
pub(crate) unsafe fn copy_from_ffi(
    ptr: *const crate::abi::CNativeLinkPreview,
) -> Option<NativeLinkPreview> {
    if ptr.is_null() {
        return None;
    }
    let native = unsafe { &*ptr };
    unsafe fn text(ptr: *const std::ffi::c_char, len: usize, limit: usize) -> Option<Arc<str>> {
        if len > limit {
            return None;
        }
        if len == 0 {
            return Some("".into());
        }
        if ptr.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        Some(std::str::from_utf8(bytes).ok()?.into())
    }
    let matched_text = unsafe {
        text(
            native.matched_text,
            native.matched_text_len,
            NATIVE_MATCHED_TEXT_LIMIT,
        )?
    };
    let title = unsafe { text(native.title, native.title_len, NATIVE_TITLE_LIMIT)? };
    let description = unsafe {
        text(
            native.description,
            native.description_len,
            NATIVE_DESCRIPTION_LIMIT,
        )?
    };
    let jpeg_thumbnail = if native.jpeg_thumbnail_len == 0
        || native.jpeg_thumbnail_len > NATIVE_THUMBNAIL_LIMIT
    {
        Vec::new()
    } else {
        if native.jpeg_thumbnail.is_null() {
            return None;
        }
        unsafe {
            std::slice::from_raw_parts(native.jpeg_thumbnail, native.jpeg_thumbnail_len).to_vec()
        }
    };
    NativeLinkPreview {
        matched_text,
        title,
        description,
        jpeg_thumbnail,
    }
    .bounded()
}
