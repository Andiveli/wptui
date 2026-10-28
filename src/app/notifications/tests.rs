use std::sync::Arc;

use super::*;
use crate::app::test_support::FixedClock;

fn message(chat: &str, text: &str) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: "id".into(),
            chat: chat.to_owned().into(),
            sender: chat.to_owned().into(),
            mentions_self: false,
            timestamp: 1,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text(text.into()),
    }
}

#[cfg(target_os = "macos")]
#[test]
fn system_sound_uses_macos_default_notification_constant() {
    assert_eq!(system_sound_name(), "NSUserNotificationDefaultSoundName");
}

#[cfg(target_os = "linux")]
#[test]
fn system_sound_uses_desktop_sound_theme_name() {
    assert_eq!(system_sound_name(), "message-new-instant");
}

#[test]
fn native_notifier_selects_sound_from_settings_without_audio_hardware() {
    let directory = tempfile::tempdir().unwrap();
    let settings = directory.path().join("settings.conf");
    let notifier = NotifyRustNotifier::new(settings.clone());
    assert_eq!(notifier.sound_choice(), NotificationSound::Bundled);
    std::fs::write(&settings, "notification_sound=relative.wav\n").unwrap();
    assert_eq!(notifier.sound_choice(), NotificationSound::Bundled);
    std::fs::write(&settings, "notification_sound=/sounds/alert.wav\n").unwrap();
    assert_eq!(
        notifier.sound_choice(),
        NotificationSound::Custom("/sounds/alert.wav".into())
    );
    std::fs::write(&settings, "notification_sound=system\n").unwrap();
    assert_eq!(notifier.sound_choice(), NotificationSound::System);
}

#[test]
fn native_notifier_keeps_selected_legacy_sound_when_config_appears_later() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("settings.conf");
    let config = directory.path().join("config.conf");
    std::fs::write(&legacy, "notification_sound=/sounds/legacy.ogg\n").unwrap();
    let notifier = NotifyRustNotifier::new(legacy);
    std::fs::write(&config, "notification_sound=system\n").unwrap();
    assert_eq!(
        notifier.sound_choice(),
        NotificationSound::Custom("/sounds/legacy.ogg".into())
    );
}

#[test]
fn stale_notices_select_no_system_hint_or_direct_audio() {
    for choice in [
        NotificationSound::System,
        NotificationSound::Bundled,
        NotificationSound::Custom("/sounds/alert.ogg".into()),
    ] {
        assert_eq!(selected_sound(false, choice.clone()), None);
        assert_eq!(selected_sound(true, choice.clone()), Some(choice));
    }
}

#[test]
fn bundled_sound_decodes_without_audio_hardware() {
    let mut decoded = rodio::Decoder::try_from(std::io::Cursor::new(ATTENTION_WAV)).unwrap();
    assert!(decoded.next().is_some());
}

#[test]
fn custom_sound_decodes_wav_mp3_and_ogg_without_audio_hardware() {
    for extension in ["wav", "mp3", "ogg"] {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(format!("notification-tone.{extension}"));
        let mut decoded = rodio::Decoder::try_from(File::open(&fixture).unwrap()).unwrap();
        assert!(decoded.next().is_some(), "no samples in {extension}");
    }
}

#[test]
fn missing_custom_audio_fails_without_panicking_or_waiting_for_a_device() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.ogg");
    assert!(play_sound(NotificationSound::Custom(missing)).is_err());
}

#[test]
fn eligibility_skips_status_and_own_messages() {
    assert!(notification_eligibility(
        &message("chat@g.us", "incoming"),
        None
    ));

    let mut own = message("chat@g.us", "own");
    own.info.is_from_me = true;
    assert!(!notification_eligibility(&own, None));
    assert!(!notification_eligibility(
        &message(STATUS_BROADCAST_CHAT, "status"),
        None
    ));
}

#[test]
fn self_mention_semantics_do_not_turn_an_own_echo_into_a_notification() {
    let mut own = message("chat@g.us", "@self");
    own.info.is_from_me = true;
    own.info.mentions_self = true;
    assert!(!notification_eligibility(&own, None));
}

#[test]
fn eligibility_suppresses_only_the_open_chat() {
    let open_chat = wr::JID::from("open@g.us".to_owned());
    let highlighted_chat = message("highlighted@g.us", "incoming");
    let open_message = message("open@g.us", "incoming");

    assert!(notification_eligibility(
        &highlighted_chat,
        Some(&open_chat)
    ));
    assert!(!notification_eligibility(&open_message, Some(&open_chat)));
    assert!(notification_eligibility(
        &message("other@g.us", "incoming"),
        Some(&open_chat)
    ));
}

#[test]
fn mute_policy_excludes_expired_boundary() {
    assert!(notification_is_muted(true, 1_001, 1_000));
    assert!(!notification_is_muted(true, 1_000, 1_000));
    assert!(!notification_is_muted(false, 2_000, 1_000));
}

#[test]
fn projection_preserves_untrusted_message_text() {
    let projection = notification_projection(
        &message("alice@s.whatsapp.net", "ignored"),
        "Alice".into(),
        true,
    );
    assert_eq!(projection.summary.as_ref(), "Alice");
    assert_eq!(projection.body, "ignored");

    let message = wr::Message {
        message: wr::MessageContent::Text("こんにちは\n\"quoted\"\u{0007}".into()),
        ..message("alice@s.whatsapp.net", "ignored")
    };
    let projection = notification_projection(&message, Arc::from("名前\n\"sender\""), true);
    assert_eq!(projection.summary.as_ref(), "名前\n\"sender\"");
    assert_eq!(projection.body, "こんにちは\n\"quoted\"\u{0007}");
}

#[test]
fn now_or_uses_clock_or_fallback() {
    assert_eq!(now_or(0, &FixedClock(Some(42))), 42);
    assert_eq!(now_or(7, &FixedClock(None)), 7);
}
