use std::path::Path;

use super::App;
use whatsrust as wr;

impl App<'_> {
    pub(crate) fn open_selected_url(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let urls = self
            .selected_message()
            .map(message_urls)
            .unwrap_or_default();
        match urls.len() {
            0 => self.open_selected_document(),
            1 => self.launch_url(&urls[0]),
            _ => self.url_picker = Some((urls, 0)),
        }
    }

    fn open_selected_document(&mut self) {
        let Some(message) = self.selected_message() else {
            return self.unavailable("Open is not available");
        };
        let wr::MessageContent::File(file) = &message.message else {
            return self.unavailable("Open is not available");
        };
        if !matches!(file.kind, wr::FileKind::Document) {
            return self.unavailable("Open is not available");
        }
        if !matches!(
            self.metadata.get(&message.info.id),
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloaded | crate::app::FileMeta::Loaded
            ))
        ) {
            return self.unavailable("Media is not downloaded");
        }
        let opener =
            match crate::media::document_opener_from_environment(Path::new(file.path.as_ref())) {
                Ok(opener) => opener,
                Err(_) => {
                    return self.unavailable(
                        "Set WPTUI_PDF_VIEWER or WPTUI_DOCUMENT_VIEWER to one executable name",
                    );
                }
            };
        match crate::media::plan_media_launch(
            Some(&self.media_path),
            Some(&opener),
            Some(Path::new(file.path.as_ref())),
        ) {
            Ok(Some(plan)) => {
                match crate::media::execute_launch(&plan, self.launch_executor.as_mut()) {
                    Ok(()) => {
                        self.action_notice = Some(crate::app::actions::ActionNotice::Unavailable(
                            "Document viewer started".into(),
                        ))
                    }
                    Err(error) => {
                        self.unavailable(&format!("Could not start document viewer: {error:?}"))
                    }
                }
            }
            Ok(None) | Err(_) => self.unavailable("Media launch is unavailable"),
        }
    }

    pub(crate) fn move_url_picker(&mut self, delta: isize) {
        if let Some((urls, selected)) = &mut self.url_picker {
            *selected = selected
                .saturating_add_signed(delta)
                .min(urls.len().saturating_sub(1));
        }
    }

    pub(crate) fn confirm_url_picker(&mut self) {
        let Some((urls, selected)) = self.url_picker.take() else {
            return;
        };
        if let Some(url) = urls.get(selected) {
            self.launch_url(url);
        } else {
            self.unavailable("Open is not available");
        }
    }

    fn launch_url(&mut self, url: &str) {
        let plan = crate::url::url_launch_plan(url);
        if self.url_opener.open(&plan).is_err() {
            self.unavailable("Could not open URL");
        }
    }
}

fn message_urls(message: &wr::Message) -> Vec<String> {
    let text = match &message.message {
        wr::MessageContent::Text(text) => Some(text.as_ref()),
        wr::MessageContent::File(file) => file.caption.as_deref(),
        wr::MessageContent::ViewOnceUnavailable => None,
    };
    text.map(crate::url::extract_openable_urls)
        .unwrap_or_default()
}
