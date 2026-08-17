use std::path::Path;

use crate::app::composer::Composer;
use crate::clipboard::{self, ClipboardError, ClipboardPaste};

/// Applies a clipboard payload to the composer without changing existing text or attachments.
pub fn apply_clipboard_paste(
    composer: &mut Composer<'_>,
    media_path: &Path,
    paste: Result<ClipboardPaste, ClipboardError>,
) -> Result<(), ClipboardError> {
    match paste? {
        ClipboardPaste::Text(text) => composer.insert_text(&text),
        ClipboardPaste::Paths(paths) => {
            for path in paths {
                let kind = clipboard::file_kind(&path);
                composer.queue_attachment(path.to_string_lossy().into_owned().into(), kind);
            }
        }
        ClipboardPaste::Png(png) => {
            let path = clipboard::persist_png(media_path, &png)?;
            composer.queue_attachment(
                path.to_string_lossy().into_owned().into(),
                whatsrust::FileKind::Image,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn applies_text_without_replacing_existing_composer_content() {
        let mut composer = Composer::default();
        composer.insert_text("draft ");

        apply_clipboard_paste(
            &mut composer,
            Path::new("/unused"),
            Ok(ClipboardPaste::Text("message".into())),
        )
        .unwrap();

        assert_eq!(composer.text(), "draft message");
    }

    #[test]
    fn queues_paths_with_their_detected_file_kind() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("image.png");
        fs::write(&path, []).unwrap();
        let mut composer = Composer::default();

        apply_clipboard_paste(
            &mut composer,
            directory.path(),
            Ok(ClipboardPaste::Paths(vec![path.clone()])),
        )
        .unwrap();

        assert_eq!(composer.pending.len(), 1);
        assert_eq!(composer.pending[0].path.as_ref(), path.to_string_lossy());
        assert!(matches!(
            composer.pending[0].kind,
            whatsrust::FileKind::Image
        ));
    }

    #[test]
    fn persists_png_payloads_as_image_attachments() {
        let directory = tempdir().unwrap();
        let mut composer = Composer::default();

        apply_clipboard_paste(
            &mut composer,
            directory.path(),
            Ok(ClipboardPaste::Png(vec![1, 2, 3])),
        )
        .unwrap();

        assert_eq!(composer.pending.len(), 1);
        assert!(composer.pending[0].path.ends_with(".png"));
        assert!(matches!(
            composer.pending[0].kind,
            whatsrust::FileKind::Image
        ));
    }

    #[test]
    fn clipboard_errors_leave_composer_state_unchanged() {
        let mut composer = Composer::default();
        composer.insert_text("draft message");
        composer.queue_attachment("existing.pdf".into(), whatsrust::FileKind::Document);

        let result = apply_clipboard_paste(
            &mut composer,
            Path::new("/unused"),
            Err(ClipboardError::EmptyText),
        );

        assert_eq!(result, Err(ClipboardError::EmptyText));
        assert_eq!(composer.text(), "draft message");
        assert_eq!(composer.pending.len(), 1);
    }
}
