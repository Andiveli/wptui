use crate::app::App;
use crate::app::events::AppEvent;

impl App<'_> {
    pub(crate) fn handle_send_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::OutboundSendSucceeded {
                local_send_id,
                message,
            } => self.complete_text_send(local_send_id, message),
            AppEvent::OutboundSendFailed { local_send_id } => self.fail_text_send(local_send_id),
            _ => unreachable!("runtime_loop must route only Send events to handle_send_event"),
        }
    }
}
