use crate::app::presence_diagnostics_port::RawPresenceDiagnosticsPort;

pub struct WhatsRustRawPresenceDiagnostics;

impl RawPresenceDiagnosticsPort for WhatsRustRawPresenceDiagnostics {
    fn drain(&self) -> Option<String> {
        whatsrust::drain_raw_presence_diagnostics()
    }
}
