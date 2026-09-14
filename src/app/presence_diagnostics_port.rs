pub trait RawPresenceDiagnosticsPort {
    fn drain(&self) -> Option<String>;
}
