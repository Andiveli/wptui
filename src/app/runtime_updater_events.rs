use crate::app::App;
use crate::app::events::AppEvent;

impl App<'_> {
    pub(crate) fn handle_updater_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::UpdateAvailable(version) => {
                self.update_notice = Some(version);
                true
            }
            _ => {
                unreachable!("runtime_loop must route only Updater events to handle_updater_event")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::events::AppEvent;
    use crate::app::test_support::TestApp;

    #[test]
    fn update_available_sets_notice_and_requests_redraw() {
        let mut app = TestApp::new();

        assert!(app.handle_updater_event(AppEvent::UpdateAvailable("1.2.3".to_owned())));
        assert_eq!(app.update_notice.as_deref(), Some("1.2.3"));
    }
}
