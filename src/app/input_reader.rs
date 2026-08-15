use std::sync::{Arc, Condvar, Mutex, mpsc::Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use log::error;
use ratatui::crossterm::event;

use crate::app::events::AppInput;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Running,
    Stopped,
}

/// Owns the terminal input reader thread and its shutdown lifecycle.
pub struct InputReader {
    control: Arc<(Mutex<State>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl InputReader {
    pub(crate) fn new() -> Self {
        Self {
            control: Arc::new((Mutex::new(State::Running), Condvar::new())),
            thread: None,
        }
    }

    pub(crate) fn start(&mut self, tx: Sender<AppInput>) {
        let control = Arc::clone(&self.control);
        self.thread = Some(thread::spawn(move || {
            loop {
                {
                    let (state_lock, _) = &*control;
                    let state = state_lock.lock().unwrap();
                    if *state == State::Stopped {
                        return;
                    }
                }

                match event::poll(Duration::from_millis(50)) {
                    Ok(true) => match event::read() {
                        Ok(event) => {
                            if let Err(e) = tx.send(AppInput::Terminal(event)) {
                                error!("Failed to send terminal event: {:?}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Failed to read terminal event: {e}");
                            thread::sleep(Duration::from_millis(50));
                        }
                    },
                    Ok(false) => {}
                    Err(e) => {
                        error!("Failed to poll terminal events: {e}");
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        }));
    }

    pub(crate) fn stop(&mut self) {
        let (state_lock, state_changed) = &*self.control;
        let mut state = state_lock.lock().unwrap();
        *state = State::Stopped;
        state_changed.notify_all();
        drop(state);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for InputReader {
    fn drop(&mut self) {
        self.stop();
    }
}
