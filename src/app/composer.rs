use std::sync::Arc;

use ratatui_textarea::TextArea;
use whatsrust as wr;

use crate::app::actions::ComposerAction;
#[derive(Clone, Debug)]
pub struct PendingAttachment {
    pub path: Arc<str>,
    pub kind: wr::FileKind,
}

impl PendingAttachment {
    pub fn new(path: Arc<str>, kind: wr::FileKind) -> Self {
        Self { path, kind }
    }

    pub fn display_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[derive(Debug)]
pub enum ComposerOutcome {
    Idle,
    Submit {
        messages: Vec<wr::MessageContent>,
        quote: Option<wr::Message>,
    },
}

impl ComposerOutcome {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn text_messages(&self) -> Vec<&str> {
        match self {
            Self::Idle => Vec::new(),
            Self::Submit { messages, .. } => messages
                .iter()
                .filter_map(|message| match message {
                    wr::MessageContent::Text(text) => Some(text.as_ref()),
                    wr::MessageContent::File(_) => None,
                })
                .collect(),
        }
    }

    pub fn file_messages(&self) -> Vec<&wr::FileContent> {
        match self {
            Self::Idle => Vec::new(),
            Self::Submit { messages, .. } => messages
                .iter()
                .filter_map(|message| match message {
                    wr::MessageContent::File(file) => Some(file),
                    wr::MessageContent::Text(_) => None,
                })
                .collect(),
        }
    }
}

pub struct Composer<'a> {
    pub input: TextArea<'a>,
    pub quote: Option<wr::Message>,
    pub pending: Vec<PendingAttachment>,
    pub blocked: bool,
}

impl Default for Composer<'_> {
    fn default() -> Self {
        let mut input = TextArea::default();
        input.set_placeholder_text("Type a message...");

        Self {
            input,
            quote: None,
            pending: Vec::new(),
            blocked: false,
        }
    }
}

impl Composer<'_> {
    pub fn apply(&mut self, action: ComposerAction) -> ComposerOutcome {
        if self.blocked {
            return ComposerOutcome::Idle;
        }
        match action {
            ComposerAction::Submit => self.submit(),
            ComposerAction::InsertNewline => {
                self.input.insert_newline();
                ComposerOutcome::Idle
            }
            ComposerAction::Edit(key) => {
                self.input.input(key);
                ComposerOutcome::Idle
            }
            ComposerAction::RemoveLastAttachment => {
                self.pending.pop();
                ComposerOutcome::Idle
            }
            ComposerAction::CancelReply => {
                self.quote = None;
                ComposerOutcome::Idle
            }
            ComposerAction::StartEdit | ComposerAction::Paste => ComposerOutcome::Idle,
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        if self.blocked {
            return;
        }
        self.input.insert_str(text);
    }

    pub fn replace_text(&mut self, text: &str) {
        self.clear_text();
        self.insert_text(text);
    }

    pub fn clear_text(&mut self) {
        self.input.select_all();
        self.input.delete_next_char();
    }

    pub fn text(&self) -> String {
        self.input.lines().join("\n")
    }

    pub fn queue_attachment(&mut self, path: Arc<str>, kind: wr::FileKind) {
        if self.blocked {
            return;
        }
        self.pending.push(PendingAttachment::new(path, kind));
    }

    pub fn set_blocked(&mut self, blocked: bool) {
        self.blocked = blocked;
        if blocked {
            self.clear_text();
            self.quote = None;
            self.pending.clear();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.input.lines().iter().all(String::is_empty) && self.pending.is_empty()
    }

    fn submit(&mut self) -> ComposerOutcome {
        let text: Arc<str> = self.input.lines().join("\n").into();
        if text.trim().is_empty() && self.pending.is_empty() {
            return ComposerOutcome::Idle;
        }

        let messages = if self.pending.is_empty() {
            vec![wr::MessageContent::Text(text)]
        } else {
            self.pending
                .drain(..)
                .enumerate()
                .map(|(index, attachment)| {
                    wr::MessageContent::File(wr::FileContent {
                        kind: attachment.kind,
                        path: attachment.path,
                        file_id: "".into(),
                        caption: (index == 0).then(|| text.clone()),
                    })
                })
                .collect()
        };

        self.input.select_all();
        self.input.delete_next_char();

        ComposerOutcome::Submit {
            messages,
            quote: self.quote.take(),
        }
    }
}
