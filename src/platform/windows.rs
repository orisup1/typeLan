use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use rdev::{listen, simulate, Event, EventType, Key};

use crate::dictionary::check_and_switch_candidates;
use crate::keymap::{key_to_english_char, key_to_hebrew_char};
use crate::types::AppControl;

pub struct AppState {
    pub keys: Vec<Key>,
    pub is_replacing: bool,
    pub buffered_keys: Vec<Key>,
}

pub fn run(en_dict: HashSet<String>, he_dict: HashSet<String>, control: Arc<AppControl>) {
    println!("Starting typeLan keyboard watcher (Windows)...");

    let en_dict_cb = en_dict.clone();
    let he_dict_cb = he_dict.clone();
    let control_cb = Arc::clone(&control);
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState {
        keys: Vec::new(),
        is_replacing: false,
        buffered_keys: Vec::new(),
    }));
    let state_cb = Arc::clone(&state);
    let injecting = Arc::new(AtomicBool::new(false));
    let injecting_cb = Arc::clone(&injecting);

    let callback = move |event: Event| {
        // Ignore the key events we generate ourselves, otherwise the listener
        // can treat them as user input and interfere with the replacement.
        if injecting_cb.load(Ordering::Relaxed) {
            return;
        }

        let mut st = state_cb.lock().unwrap();
        match event.event_type {
            EventType::KeyPress(key) => match key {
                Key::Space | Key::Return => {
                    if st.is_replacing {
                        st.buffered_keys.push(key);
                        return;
                    }

                    if !st.keys.is_empty() {
                        if !control_cb.is_enabled() {
                            st.keys.clear();
                            return;
                        }
                        let word_en: String =
                            st.keys.iter().filter_map(|&k| key_to_english_char(k)).collect();
                        let word_he: String =
                            st.keys.iter().filter_map(|&k| key_to_hebrew_char(k)).collect();
                        let switched = check_and_switch_candidates(
                            &word_en,
                            &word_he,
                            &en_dict_cb,
                            &he_dict_cb,
                        );

                        if switched {
                            control_cb.record_fix();
                            st.is_replacing = true;
                            let keys_clone = st.keys.clone();
                            let terminator = key;
                            let state_clone = Arc::clone(&state_cb);
                            injecting.store(true, Ordering::Relaxed);
                            let injecting_flag = Arc::clone(&injecting_cb);

                            thread::spawn(move || {
                                // switch_layout_to already polled until the layout
                                // change took effect, so no settle delay is needed.

                                let mut st_lock = state_clone.lock().unwrap();
                                let buf = st_lock.buffered_keys.clone();

                                // +1 for the terminator the user physically typed.
                                let delete_count = keys_clone.len() + 1 + buf.len();
                                for _ in 0..delete_count {
                                    let _ = simulate(&EventType::KeyPress(Key::Backspace));
                                    let _ = simulate(&EventType::KeyRelease(Key::Backspace));
                                    thread::sleep(Duration::from_micros(150));
                                }

                                for k in keys_clone {
                                    let _ = simulate(&EventType::KeyPress(k));
                                    let _ = simulate(&EventType::KeyRelease(k));
                                    thread::sleep(Duration::from_micros(150));
                                }

                                let _ = simulate(&EventType::KeyPress(terminator));
                                let _ = simulate(&EventType::KeyRelease(terminator));

                                for k in buf.iter() {
                                    let _ = simulate(&EventType::KeyPress(*k));
                                    let _ = simulate(&EventType::KeyRelease(*k));
                                    thread::sleep(Duration::from_micros(150));
                                }

                                st_lock.keys = buf;
                                st_lock.buffered_keys.clear();
                                st_lock.is_replacing = false;
                                injecting_flag.store(false, Ordering::Relaxed);
                            });
                        }

                        st.keys.clear();
                    }
                }
                Key::Backspace => {
                    if st.is_replacing {
                        st.buffered_keys.pop();
                    } else {
                        st.keys.pop();
                    }
                }
                Key::Tab
                | Key::Escape
                | Key::LeftArrow
                | Key::RightArrow
                | Key::UpArrow
                | Key::DownArrow
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
                | Key::Insert
                | Key::Delete => {
                    if st.is_replacing {
                        st.buffered_keys.clear();
                    } else {
                        st.keys.clear();
                    }
                }
                _ => {
                    if key_to_english_char(key).is_some() || key_to_hebrew_char(key).is_some() {
                        if st.is_replacing {
                            st.buffered_keys.push(key);
                        } else {
                            st.keys.push(key);
                        }
                    }
                }
            },
            EventType::ButtonPress(_) => {
                if st.is_replacing {
                    st.buffered_keys.clear();
                } else {
                    st.keys.clear();
                }
            }
            _ => {}
        }
    };

    println!("Listening for keyboard events. Press Space or Enter to check a word.");
    if let Err(err) = listen(callback) {
        eprintln!("Error while listening for keyboard events: {:?}", err);
    }
}

