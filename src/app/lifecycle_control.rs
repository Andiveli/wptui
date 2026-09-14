pub(crate) type QrCallback = Box<dyn FnMut(String) + 'static>;

pub(crate) trait LifecycleControl: 'static {
    fn new_client(&self, db_path: &str);
    fn connect(&self, on_qr: QrCallback);
    fn pair_phone(&self, phone: &str) -> String;
    fn disconnect(&self);
    fn logout(&self);
}
