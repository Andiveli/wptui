use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use notify_rust::Notification;
use whatsrust as wr;

use super::STATUS_BROADCAST_CHAT;

pub trait Clock {
    fn unix_seconds(&self) -> Option<i64>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_seconds(&self) -> Option<i64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs() as i64)
    }
}

pub struct NotificationProjection {
    pub summary: Arc<str>,
    pub body: String,
}

pub trait Notifier {
    fn show(&self, notification: &NotificationProjection) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct NotifyRustNotifier;

impl Notifier for NotifyRustNotifier {
    fn show(&self, notification: &NotificationProjection) -> Result<(), String> {
        Notification::new()
            .summary(&notification.summary)
            .body(&notification.body)
            .show()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn notification_eligibility(message: &wr::Message) -> bool {
    !message.info.is_from_me && message.info.chat.0.as_ref() != STATUS_BROADCAST_CHAT
}

pub(crate) fn notification_is_muted(found: bool, muted_until: i64, now: i64) -> bool {
    found && muted_until > now
}

pub(crate) fn notification_projection(
    message: &wr::Message,
    summary: Arc<str>,
) -> NotificationProjection {
    let body = match &message.message {
        wr::MessageContent::Text(text) => text.to_string(),
        wr::MessageContent::File(file) => file.caption.as_ref().map_or_else(
            || match file.kind {
                wr::FileKind::Image => "Sent an image".to_string(),
                wr::FileKind::Video => "Sent a video".to_string(),
                wr::FileKind::Audio => "Sent an audio message".to_string(),
                wr::FileKind::Document => "Sent a document".to_string(),
                wr::FileKind::Sticker => "Sent a sticker".to_string(),
            },
            ToString::to_string,
        ),
    };
    NotificationProjection { summary, body }
}

pub fn now_or(fallback: i64, clock: &dyn Clock) -> i64 {
    clock.unix_seconds().unwrap_or(fallback)
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;
