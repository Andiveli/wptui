pub trait CallbackTranslator<T> {
    unsafe fn to_rust(c_value: T) -> Self;
}

macro_rules! setup_handler {
    ($fn_name:ident, $c_func:ident) => {
        setup_handler!($fn_name, $c_func,);
    };
    (
        $fn_name:ident,
        $c_func:ident,
        $(
            $param_name:ident : $c_type:ty => $rs_type:ty
        ),* $(,)?
    ) => {
        pub fn $fn_name<F>(callback: F)
        where
            F: FnMut($($rs_type),*) + 'static,
        {
            type CallbackType = dyn FnMut($($rs_type),*);
            let callback: Box<CallbackType> = Box::new(callback);
            let callback_state = Box::new(std::sync::Arc::new(
                std::sync::Mutex::new(callback),
            ));
            let user_data = Box::into_raw(callback_state) as *mut std::ffi::c_void;

            extern "C" fn shim(
                $( $param_name: $c_type, )*
                user_data: *mut std::ffi::c_void
            ) {
                unsafe {
                    let callback_state = &*(user_data
                        as *const std::sync::Arc<
                            std::sync::Mutex<Box<CallbackType>>,
                        >);
                    let closure = std::sync::Arc::clone(callback_state);
                    let mut guard = closure.lock().unwrap();
                    guard($(
                        <$rs_type as $crate::callbacks::CallbackTranslator<$c_type>>::to_rust(
                            $param_name,
                        ),
                    )*);
                }
            }

            unsafe {
                $c_func(shim, user_data);
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::CallbackTranslator;
    use std::{
        ffi::c_void,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    type TestCallback = extern "C" fn(usize, *mut c_void);
    static REGISTERED: Mutex<Option<(TestCallback, usize)>> = Mutex::new(None);

    unsafe fn register_test_callback(callback: TestCallback, user_data: *mut c_void) {
        *REGISTERED.lock().unwrap() = Some((callback, user_data as usize));
    }

    setup_handler!(set_test_handler, register_test_callback, value: usize => usize);

    impl super::CallbackTranslator<usize> for usize {
        unsafe fn to_rust(value: usize) -> Self {
            value
        }
    }

    #[test]
    fn stable_user_data_delivers_every_concurrent_callback_once() {
        const THREADS: usize = 16;
        const CALLS_PER_THREAD: usize = 1_000;
        let delivered = Arc::new(AtomicUsize::new(0));
        let sum = Arc::new(AtomicUsize::new(0));
        set_test_handler({
            let delivered = Arc::clone(&delivered);
            let sum = Arc::clone(&sum);
            move |value| {
                delivered.fetch_add(1, Ordering::Relaxed);
                sum.fetch_add(value, Ordering::Relaxed);
            }
        });
        let (callback, user_data) = REGISTERED.lock().unwrap().unwrap();

        let threads: Vec<_> = (0..THREADS)
            .map(|_| {
                thread::spawn(move || {
                    for _ in 0..CALLS_PER_THREAD {
                        callback(1, user_data as *mut c_void);
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }

        let expected = THREADS * CALLS_PER_THREAD;
        assert_eq!(delivered.load(Ordering::Relaxed), expected);
        assert_eq!(sum.load(Ordering::Relaxed), expected);
    }
}
