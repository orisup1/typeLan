use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead};

// ── macOS-only imports ────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
use core_foundation::base::TCFType;
#[cfg(target_os = "macos")]
use core_foundation::string::CFString;
#[cfg(target_os = "macos")]
use core_foundation_sys::base::CFTypeRef;
#[cfg(target_os = "macos")]
use core_foundation_sys::string::CFStringRef;

#[cfg(target_os = "macos")]
#[repr(C)]
struct __TISInputSource;
#[cfg(target_os = "macos")]
type TISInputSourceRef = *mut __TISInputSource;

#[cfg(target_os = "macos")]
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyInputSourceForLanguage(language: CFStringRef) -> TISInputSourceRef;
    fn TISSelectInputSource(source: TISInputSourceRef) -> i32;
    fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;
    fn CFRelease(cf: CFTypeRef);
}

// ── Shared types ──────────────────────────────────────────────────────────────
#[derive(Clone, Copy, Debug, PartialEq)]
enum Language {
    English,
    Hebrew,
}

fn load_dictionary(path: &str) -> io::Result<HashSet<String>> {
    let file = File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut dict = HashSet::new();
    for line in reader.lines() {
        let word = line?.trim().to_lowercase();
        if !word.is_empty() {
            dict.insert(word);
        }
    }
    Ok(dict)
}

fn check_and_switch_candidates(
    word_en: &str,
    word_he: &str,
    en_dict: &HashSet<String>,
    he_dict: &HashSet<String>,
) -> bool {
    let word_en_lower = word_en.to_lowercase();
    let word_he_lower = word_he.to_lowercase();
    let is_in_en = !word_en_lower.is_empty() && en_dict.contains(&word_en_lower);
    let is_in_he = !word_he_lower.is_empty() && he_dict.contains(&word_he_lower);

    let final_en = is_in_en && !is_in_he;
    let final_he = is_in_he && !is_in_en;

    let target_lang = if final_en {
        Some(Language::English)
    } else if final_he {
        Some(Language::Hebrew)
    } else {
        None
    };

    println!("{}", word_en);
    println!("English: {}", final_en);
    println!("Hebrew: {}", final_he);

    if let Some(lang) = target_lang {
        let switched = switch_layout_to(lang);
        println!("switching: {}", if switched { "True" } else { "False" });
        switched
    } else {
        println!("switching: False");
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS backend
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
fn main() {
    use rdev::{listen, simulate, Event, EventType, Key};
    use std::sync::{Arc, Mutex};

    println!("Starting typeLan keyboard watcher (macOS)...");

    let en_dict = load_dictionary("en_dict.txt").unwrap_or_else(|e| {
        eprintln!("Warning: Could not load English dictionary: {}", e);
        HashSet::new()
    });
    let he_dict = load_dictionary("he_dict.txt").unwrap_or_else(|e| {
        eprintln!("Warning: Could not load Hebrew dictionary: {}", e);
        HashSet::new()
    });

    if en_dict.is_empty() || he_dict.is_empty() {
        eprintln!("Fatal: At least one dictionary must be loaded. Shutting down.");
        return;
    }

    let current_keys: Arc<Mutex<Vec<Key>>> = Arc::new(Mutex::new(Vec::new()));
    let en_dict_cb = en_dict.clone();
    let he_dict_cb = he_dict.clone();
    let keys_cb = Arc::clone(&current_keys);

    let callback = move |event: Event| {
        let mut keys = keys_cb.lock().unwrap();
        match event.event_type {
            EventType::KeyPress(key) => match key {
                Key::Space | Key::Return => {
                    if !keys.is_empty() {
                        let word_en: String =
                            keys.iter().filter_map(|&k| key_to_english_char(k)).collect();
                        let word_he: String =
                            keys.iter().filter_map(|&k| key_to_hebrew_char(k)).collect();
                        let switched = check_and_switch_candidates(
                            &word_en, &word_he, &en_dict_cb, &he_dict_cb,
                        );
                        if switched {
                            let keys_clone = keys.clone();
                            std::thread::spawn(move || {
                                use std::thread;
                                use std::time::Duration;
                                thread::sleep(Duration::from_millis(50));
                                let delete_count = keys_clone.len() + 1;
                                for _ in 0..delete_count {
                                    let _ = simulate(&EventType::KeyPress(Key::Backspace));
                                    let _ = simulate(&EventType::KeyRelease(Key::Backspace));
                                    thread::sleep(Duration::from_millis(1));
                                }
                                thread::sleep(Duration::from_millis(30));
                                for k in keys_clone {
                                    let _ = simulate(&EventType::KeyPress(k));
                                    let _ = simulate(&EventType::KeyRelease(k));
                                    thread::sleep(Duration::from_millis(1));
                                }
                                let _ = simulate(&EventType::KeyPress(Key::Space));
                                let _ = simulate(&EventType::KeyRelease(Key::Space));
                            });
                        }
                        keys.clear();
                    }
                }
                Key::Backspace => {
                    keys.pop();
                }
                _ => {
                    if key_to_english_char(key).is_some() || key_to_hebrew_char(key).is_some() {
                        keys.push(key);
                    }
                }
            },
            _ => {}
        }
    };

    println!("Listening for keyboard events. Press Space or Enter to check a word.");
    if let Err(err) = listen(callback) {
        eprintln!("Error while listening for keyboard events: {:?}", err);
    }
}

#[cfg(target_os = "macos")]
fn switch_layout_to(lang: Language) -> bool {
    let code = match lang {
        Language::English => "en",
        Language::Hebrew => "he",
    };
    unsafe {
        let cf_lang = CFString::new(code);
        let src = TISCopyInputSourceForLanguage(cf_lang.as_concrete_TypeRef());
        if src.is_null() {
            eprintln!("No input source found for language code '{}'", code);
            return false;
            return false;
        }
        let current_src = TISCopyCurrentKeyboardInputSource();
        let mut switched = false;
        if current_src.is_null()
            || core_foundation_sys::base::CFEqual(
                src as CFTypeRef,
                current_src as CFTypeRef,
            ) == 0
        {
            let status = TISSelectInputSource(src);
            if status != 0 {
                eprintln!(
                    "TISSelectInputSource failed for '{}' with status {}",
                    code, status
                );
            } else {
                switched = true;
            }
        }
        if !current_src.is_null() {
            CFRelease(current_src as CFTypeRef);
        }
        CFRelease(src as CFTypeRef);
        switched
    }
}

#[cfg(target_os = "linux")]
struct AppState {
    keys: Vec<evdev::KeyCode>,
    last_event_time: std::time::Instant,
    last_keycode: Option<evdev::KeyCode>,
}

#[cfg(target_os = "linux")]
fn main() {
    use evdev::{uinput::VirtualDevice, AttributeSet, Device, EventSummary, KeyCode};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    println!("Starting typeLan keyboard watcher (Linux/Wayland)...");

    let en_dict = load_dictionary("en_dict.txt").unwrap_or_else(|e| {
        eprintln!("Warning: Could not load English dictionary: {}", e);
        HashSet::new()
    });
    let he_dict = load_dictionary("he_dict.txt").unwrap_or_else(|e| {
        eprintln!("Warning: Could not load Hebrew dictionary: {}", e);
        HashSet::new()
    });

    if en_dict.is_empty() || he_dict.is_empty() {
        eprintln!("Fatal: At least one dictionary must be loaded. Shutting down.");
        return;
    }

    // Persistent virtual device strictly for INJECTING backspaces and corrected words.
    // We create this once so Wayland has time to recognize it before we need to type.
    let mut all_keys = AttributeSet::<KeyCode>::new();
    for code in 0u16..=249 {
        all_keys.insert(KeyCode::new(code));
    }

    let builder = match VirtualDevice::builder() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to create injector builder: {}", e);
            return;
        }
    };

    let injector = match builder.name("typeLan-injector").with_keys(&all_keys) {
        Ok(b) => match b.build() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to build injector device: {}", e);
                return;
            }
        },
        Err(e) => {
            eprintln!("Failed to configure injector device: {}", e);
            return;
        }
    };

    // Allow time for the OS/compositor to detect the new injection device
    thread::sleep(Duration::from_millis(300));
    let injector = Arc::new(Mutex::new(injector));

    // Find all physical keyboard devices
    let keyboard_paths: Vec<std::path::PathBuf> = evdev::enumerate()
        .filter_map(|(path, dev)| {
            if dev.name() == Some("typeLan-injector") {
                return None;
            }
            if dev
                .supported_keys()
                .map_or(false, |k| k.contains(KeyCode::KEY_A))
            {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    if keyboard_paths.is_empty() {
        eprintln!("No keyboard devices found. Make sure you are in the 'input' group.");
        return;
    }

    println!("Found {} keyboard device(s).", keyboard_paths.len());

    let state = Arc::new(Mutex::new(AppState {
        keys: Vec::new(),
        last_event_time: Instant::now(),
        last_keycode: None,
    }));

    let en_dict = Arc::new(en_dict);
    let he_dict = Arc::new(he_dict);

    let mut handles = vec![];

    for path in keyboard_paths {
        let state = Arc::clone(&state);
        let en_dict = Arc::clone(&en_dict);
        let he_dict = Arc::clone(&he_dict);
        let injector = Arc::clone(&injector);
        let path_clone = path.clone();

        let handle = thread::spawn(move || {
            let mut dev = match Device::open(&path_clone) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Could not open {:?}: {}", path_clone, e);
                    return;
                }
            };

            println!("Passively listening on {:?}", path_clone);

            // Notice we do NOT grab the device. We let the physical device type normally.
            loop {
                let events = match dev.fetch_events() {
                    Ok(ev) => ev.collect::<Vec<_>>(),
                    Err(e) => {
                        eprintln!("Error reading {:?}: {}", path_clone, e);
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                };

                for event in events {
                    if let EventSummary::Key(_, keycode, value) = event.destructure() {
                        // Only process key presses (1)
                        if value == 1 {
                            handle_key_linux(keycode, &state, &en_dict, &he_dict, &injector);
                        }
                    }
                }
            }
        });
        handles.push(handle);
    }

    println!("Listening for keyboard events. Press Space or Enter to check a word.");
    for h in handles {
        let _ = h.join();
    }
}

#[cfg(target_os = "linux")]
fn handle_key_linux(
    key: evdev::KeyCode,
    state_mutex: &std::sync::Arc<std::sync::Mutex<AppState>>,
    en_dict: &HashSet<String>,
    he_dict: &HashSet<String>,
    injector: &std::sync::Arc<std::sync::Mutex<evdev::uinput::VirtualDevice>>,
) {
    use evdev::KeyCode as KC;
    use std::time::{Duration, Instant};

    let mut st = state_mutex.lock().unwrap();

    // Deduplicate exact same keypress across multiple event nodes within 5ms
    let now = Instant::now();
    if st.last_keycode == Some(key) && now.duration_since(st.last_event_time) < Duration::from_millis(5) {
        return;
    }
    st.last_event_time = now;
    st.last_keycode = Some(key);

    match key {
        KC::KEY_SPACE | KC::KEY_ENTER | KC::KEY_KPENTER => {
            if !st.keys.is_empty() {
                let word_en: String = st.keys.iter().filter_map(|&k| evkey_to_english_char(k)).collect();
                let word_he: String = st.keys.iter().filter_map(|&k| evkey_to_hebrew_char(k)).collect();

                let switched = check_and_switch_candidates(&word_en, &word_he, en_dict, he_dict);

                if switched {
                    let keys_clone = st.keys.clone();
                    let terminator = key;
                    let injector_clone = std::sync::Arc::clone(injector);
                    
                    std::thread::spawn(move || {
                        linux_replace_word(keys_clone, terminator, &injector_clone);
                    });
                }
                st.keys.clear();
            }
        }
        KC::KEY_BACKSPACE => {
            st.keys.pop();
        }
        _ => {
            if evkey_to_english_char(key).is_some() || evkey_to_hebrew_char(key).is_some() {
                st.keys.push(key);
            }
        }
    }
}

/// After a layout switch, delete the mistyped word and retype it in the new layout.
#[cfg(target_os = "linux")]
fn linux_replace_word(
    keys: Vec<evdev::KeyCode>,
    terminator: evdev::KeyCode,
    injector: &std::sync::Arc<std::sync::Mutex<evdev::uinput::VirtualDevice>>,
) {
    use evdev::{EventType, InputEvent, KeyCode as KC, SynchronizationCode};
    use std::thread;
    use std::time::Duration;

    let emit = |kc: KC, val: i32| {
        let evs = [
            InputEvent::new(EventType::KEY.0, kc.0, val),
            InputEvent::new(EventType::SYNCHRONIZATION.0, SynchronizationCode::SYN_REPORT.0, 0),
        ];
        if let Ok(mut dev) = injector.lock() {
            let _ = dev.emit(&evs);
        }
    };

    let press_release = |kc: KC| {
        emit(kc, 1);
        thread::sleep(Duration::from_millis(5));
        emit(kc, 0);
        thread::sleep(Duration::from_millis(5));
    };

    // 1. Wait a significant amount of time for the `hyprctl` layout switch to actually take effect in the compositor
    thread::sleep(Duration::from_millis(120));

    // 2. Erase the word (+1 for the terminator which the user actually physically typed)
    let delete_count = keys.len() + 1;
    for _ in 0..delete_count {
        press_release(KC::KEY_BACKSPACE);
    }
    
    // Slight pause between erase and retype
    thread::sleep(Duration::from_millis(20));

    // 3. Retype the physical keys
    for key in &keys {
        press_release(*key);
    }

    // 4. Retype the terminator
    press_release(terminator);
}

/// Switch keyboard layout via hyprctl (index 0 = English/us, index 1 = Hebrew/il)
#[cfg(target_os = "linux")]
fn switch_layout_to(lang: Language) -> bool {
    use std::process::Command;

    // First check what layout we are currently on to avoid infinite loops and unnecessary delays
    if let Ok(output) = Command::new("hyprctl").args(&["devices", "-j"]).output() {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            let mut is_currently_hebrew = false;
            let mut is_currently_english = false;
            
            for block in stdout.split('{') {
                if block.contains("\"main\": true") || block.contains("\"main\":true") {
                    if let Some(idx) = block.find("\"active_keymap\":") {
                        let remainder = &block[idx+16..];
                        if let Some(start) = remainder.find('"') {
                            let val_remainder = &remainder[start+1..];
                            if let Some(end) = val_remainder.find('"') {
                                let keymap = val_remainder[..end].to_lowercase();
                                if keymap.contains("hebrew") || keymap.contains("il") {
                                    is_currently_hebrew = true;
                                } else if keymap.contains("english") || keymap.contains("us") {
                                    is_currently_english = true;
                                }
                            }
                        }
                    }
                }
            }

            if lang == Language::English && is_currently_english {
                return false; // Already in English
            }
            if lang == Language::Hebrew && is_currently_hebrew {
                return false; // Already in Hebrew
            }
        }
    }

    let index = match lang {
        Language::English => "0",
        Language::Hebrew => "1",
    };
    match Command::new("hyprctl")
        .args(&["switchxkblayout", "all", index])
        .status()
    {
        Ok(status) => status.success(),
        Err(e) => {
            eprintln!("Failed to switch layout using hyprctl: {}", e);
            false
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Linux key → character mappings
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
fn evkey_to_english_char(key: evdev::KeyCode) -> Option<char> {
    use evdev::KeyCode as K;
    match key {
        K::KEY_A => Some('a'), K::KEY_B => Some('b'), K::KEY_C => Some('c'),
        K::KEY_D => Some('d'), K::KEY_E => Some('e'), K::KEY_F => Some('f'),
        K::KEY_G => Some('g'), K::KEY_H => Some('h'), K::KEY_I => Some('i'),
        K::KEY_J => Some('j'), K::KEY_K => Some('k'), K::KEY_L => Some('l'),
        K::KEY_M => Some('m'), K::KEY_N => Some('n'), K::KEY_O => Some('o'),
        K::KEY_P => Some('p'), K::KEY_Q => Some('q'), K::KEY_R => Some('r'),
        K::KEY_S => Some('s'), K::KEY_T => Some('t'), K::KEY_U => Some('u'),
        K::KEY_V => Some('v'), K::KEY_W => Some('w'), K::KEY_X => Some('x'),
        K::KEY_Y => Some('y'), K::KEY_Z => Some('z'),
        K::KEY_1 => Some('1'), K::KEY_2 => Some('2'), K::KEY_3 => Some('3'),
        K::KEY_4 => Some('4'), K::KEY_5 => Some('5'), K::KEY_6 => Some('6'),
        K::KEY_7 => Some('7'), K::KEY_8 => Some('8'), K::KEY_9 => Some('9'),
        K::KEY_0 => Some('0'),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn evkey_to_hebrew_char(key: evdev::KeyCode) -> Option<char> {
    use evdev::KeyCode as K;
    match key {
        K::KEY_Q => Some('ק'), K::KEY_W => Some('ו'), K::KEY_E => Some('ק'),
        K::KEY_R => Some('ר'), K::KEY_T => Some('ת'), K::KEY_Y => Some('י'),
        K::KEY_U => Some('ו'), K::KEY_I => Some('ט'), K::KEY_O => Some('ם'),
        K::KEY_P => Some('פ'), K::KEY_A => Some('ש'), K::KEY_S => Some('ד'),
        K::KEY_D => Some('ג'), K::KEY_F => Some('כ'), K::KEY_G => Some('ע'),
        K::KEY_H => Some('י'), K::KEY_J => Some('ח'), K::KEY_K => Some('ל'),
        K::KEY_L => Some('ך'), K::KEY_Z => Some('ז'), K::KEY_X => Some('ס'),
        K::KEY_C => Some('ב'), K::KEY_V => Some('ה'), K::KEY_B => Some('נ'),
        K::KEY_N => Some('מ'), K::KEY_M => Some('צ'),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS key → character mappings
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
fn key_to_english_char(key: rdev::Key) -> Option<char> {
    use rdev::Key as K;
    match key {
        K::KeyA => Some('a'), K::KeyB => Some('b'), K::KeyC => Some('c'),
        K::KeyD => Some('d'), K::KeyE => Some('e'), K::KeyF => Some('f'),
        K::KeyG => Some('g'), K::KeyH => Some('h'), K::KeyI => Some('i'),
        K::KeyJ => Some('j'), K::KeyK => Some('k'), K::KeyL => Some('l'),
        K::KeyM => Some('m'), K::KeyN => Some('n'), K::KeyO => Some('o'),
        K::KeyP => Some('p'), K::KeyQ => Some('q'), K::KeyR => Some('r'),
        K::KeyS => Some('s'), K::KeyT => Some('t'), K::KeyU => Some('u'),
        K::KeyV => Some('v'), K::KeyW => Some('w'), K::KeyX => Some('x'),
        K::KeyY => Some('y'), K::KeyZ => Some('z'),
        K::Num1 => Some('1'), K::Num2 => Some('2'), K::Num3 => Some('3'),
        K::Num4 => Some('4'), K::Num5 => Some('5'), K::Num6 => Some('6'),
        K::Num7 => Some('7'), K::Num8 => Some('8'), K::Num9 => Some('9'),
        K::Num0 => Some('0'),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn key_to_hebrew_char(key: rdev::Key) -> Option<char> {
    use rdev::Key as K;
    match key {
        K::KeyQ => Some('ק'), K::KeyW => Some('ו'), K::KeyE => Some('ק'),
        K::KeyR => Some('ר'), K::KeyT => Some('ת'), K::KeyY => Some('י'),
        K::KeyU => Some('ו'), K::KeyI => Some('ט'), K::KeyO => Some('ם'),
        K::KeyP => Some('פ'), K::KeyA => Some('ש'), K::KeyS => Some('ד'),
        K::KeyD => Some('ג'), K::KeyF => Some('כ'), K::KeyG => Some('ע'),
        K::KeyH => Some('י'), K::KeyJ => Some('ח'), K::KeyK => Some('ל'),
        K::KeyL => Some('ך'), K::KeyZ => Some('ז'), K::KeyX => Some('ס'),
        K::KeyC => Some('ב'), K::KeyV => Some('ה'), K::KeyB => Some('נ'),
        K::KeyN => Some('מ'), K::KeyM => Some('צ'),
        _ => None,
    }
}
