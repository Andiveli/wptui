use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use super::preferences::{self, NotificationSound};

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
pub struct NotifyRustNotifier {
    settings_path: Option<PathBuf>,
    legacy_settings_path: Option<PathBuf>,
}

impl NotifyRustNotifier {
    pub(crate) fn new(settings_path: PathBuf) -> Self {
        Self {
            settings_path: Some(settings_path),
            legacy_settings_path: None,
        }
    }

    pub(crate) fn with_legacy_settings(mut self, path: PathBuf) -> Self {
        self.legacy_settings_path = Some(path);
        self
    }

    fn sound_choice(&self) -> NotificationSound {
        let Some(path) = self.settings_path.as_ref() else {
            return NotificationSound::System;
        };
        let path = if !path.exists() {
            self.legacy_settings_path.as_ref().unwrap_or(path)
        } else {
            path
        };
        preferences::load_notification_sound(path)
    }
}

#[cfg(target_os = "linux")]
fn system_sound_name() -> &'static str {
    "message-new-instant"
}

#[cfg(target_os = "macos")]
fn system_sound_name() -> &'static str {
    "NSUserNotificationDefaultSoundName"
}

fn play_custom_sound(path: &Path) -> Result<(), String> {
    let source = rodio::Decoder::try_from(File::open(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let stream =
        rodio::DeviceSinkBuilder::open_default_sink().map_err(|error| error.to_string())?;
    let player = rodio::Player::connect_new(&stream.mixer());
    player.append(source);
    player.sleep_until_end();
    Ok(())
}

impl Notifier for NotifyRustNotifier {
    fn show(&self, notification: &NotificationProjection) -> Result<(), String> {
        let mut desktop = Notification::new();
        desktop
            .summary(&notification.summary)
            .body(&notification.body);
        let sound = self.sound_choice();
        match &sound {
            NotificationSound::System => {
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                desktop.sound_name(system_sound_name());
            }
            NotificationSound::Custom(_) => {} // No system sound hint for custom playback.
        }
        desktop.show().map_err(|error| error.to_string())?;
        if let NotificationSound::Custom(path) = sound {
            if let Err(error) = thread::Builder::new()
                .name("notification-sound".into())
                .spawn(move || {
                    if let Err(error) = play_custom_sound(&path) {
                        log::warn!("Could not play notification sound: {error}");
                    }
                })
            {
                log::warn!("Could not start notification sound: {error}");
            }
        }
        Ok(())
    }
}

pub(crate) fn notification_eligibility(message: &wr::Message, open_chat: Option<&wr::JID>) -> bool {
    !message.info.is_from_me
        && message.info.chat.0.as_ref() != STATUS_BROADCAST_CHAT
        && open_chat != Some(&message.info.chat)
}

pub(crate) fn notification_is_muted(found: bool, muted_until: i64, now: i64) -> bool {
    found && muted_until > now
}

pub(crate) fn notification_projection(
    message: &wr::Message,
    summary: Arc<str>,
) -> NotificationProjection {
    let body = match &message.message {
        wr::MessageContent::ViewOnceUnavailable => wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION.to_owned(),
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
