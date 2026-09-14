use whatsrust as wr;

use crate::app::optimistic_text_send::{TextSendPort, TextSendRequest};

pub struct WhatsAppTextSendPort;

impl TextSendPort for WhatsAppTextSendPort {
    fn send(&mut self, request: &TextSendRequest) -> Result<(), wr::OutboundSendFailure> {
        wr::send_outbound_message(
            &request.chat,
            &request.content,
            request.quote.as_ref(),
            &request.mentions,
            request.local_send_id,
        )
    }
}
