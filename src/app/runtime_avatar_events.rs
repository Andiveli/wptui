use crate::app::App;
use crate::app::events::AppEvent;

impl App<'_> {
    pub(crate) fn handle_avatar_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::ContactAvatar(result) => self.contact_avatars.apply(result),
            AppEvent::ContactAvatarRefreshed { generation, target } => {
                self.contact_avatars.mark_refreshed(generation, target)
            }
            _ => unreachable!("runtime_loop must route only Avatar events to handle_avatar_event"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::contact_avatars::{AvatarResult, AvatarTarget};
    use crate::app::events::AppEvent;
    use crate::app::test_support::TestApp;
    use whatsrust as wr;

    fn target() -> AvatarTarget {
        AvatarTarget::Contact(wr::JID::from("avatar@example.test".to_owned()))
    }

    #[test]
    fn contact_avatar_result_preserves_apply_redraw_boolean() {
        let mut app = TestApp::new();

        assert!(
            !app.handle_avatar_event(AppEvent::ContactAvatar(AvatarResult::Failed {
                generation: 0,
                target: target(),
            }))
        );
    }

    #[test]
    fn contact_avatar_refresh_preserves_mark_refreshed_redraw_boolean() {
        let mut app = TestApp::new();

        assert!(!app.handle_avatar_event(AppEvent::ContactAvatarRefreshed {
            generation: 0,
            target: target(),
        }));
    }
}
