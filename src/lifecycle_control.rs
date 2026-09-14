use whatsrust as wr;

pub struct WhatsRustLifecycleControl;

impl crate::app::lifecycle_control::LifecycleControl for WhatsRustLifecycleControl {
    fn new_client(&self, db_path: &str) {
        wr::new_client(db_path);
    }

    fn connect(&self, on_qr: crate::app::lifecycle_control::QrCallback) {
        wr::connect(on_qr);
    }

    fn pair_phone(&self, phone: &str) -> String {
        wr::pair_phone(phone)
    }
}
