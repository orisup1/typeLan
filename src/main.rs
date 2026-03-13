use rdev::{listen, simulate, Event, EventType, Key};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead};
use std::sync::{Arc, Mutex};

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::base::CFTypeRef;
use core_foundation_sys::string::CFStringRef;

// FFI bindings to macOS Text Input Source (TIS) APIs from the Carbon framework.
#[repr(C)]
struct __TISInputSource;
type TISInputSourceRef = *mut __TISInputSource;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyInputSourceForLanguage(language: CFStringRef) -> TISInputSourceRef;
    fn TISSelectInputSource(source: TISInputSourceRef) -> i32; // OSStatus
    fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;
    fn CFRelease(cf: CFTypeRef);
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Language {
    English,
    Hebrew,
}

fn main() {
    println!("Starting typeLan keyboard watcher...");

    // language dictionaries paths
    let en_dict_path = "en_dict.txt";
    let he_dict_path = "he_dict.txt";

    // loading dictionaries
    let en_dict = load_dictionary(en_dict_path).unwrap_or_else(|e| {
        eprintln!("Warning: Could not load English dictionary: {}", e);
        HashSet::new()
    });
    let he_dict = load_dictionary(he_dict_path).unwrap_or_else(|e| {
        eprintln!("Warning: Could not load Hebrew dictionary: {}", e);
        HashSet::new()
    });

    if en_dict.is_empty() || he_dict.is_empty() {
        eprintln!("Fatal: At least one dictionary must be loaded. Shutting down.");
        return;
    }

    let current_keys = Arc::new(Mutex::new(Vec::new()));

    // Clone for the callback closure
    let en_dict_cb = en_dict.clone();
    let he_dict_cb = he_dict.clone();
    let keys_cb = Arc::clone(&current_keys);

    let callback = move |event: Event| {
        let mut keys = keys_cb.lock().unwrap();

        match event.event_type {
            EventType::KeyPress(key) => match key {
                Key::Space | Key::Return => {
                    if !keys.is_empty() {
                        let word_en: String = keys.iter().filter_map(|&k| key_to_english_char(k)).collect();
                        let word_he: String = keys.iter().filter_map(|&k| key_to_hebrew_char(k)).collect();

                        let switched = check_and_switch_candidates(
                            &word_en,
                            &word_he,
                            &en_dict_cb,
                            &he_dict_cb,
                        );

                        if switched {
                            let keys_clone = keys.clone();
                            std::thread::spawn(move || {
                                replace_word(keys_clone, key);
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
            EventType::KeyRelease(_) => {}
            _ => {}
        }
    };

    println!("Listening for keyboard events. Press Space or Enter to check a word.");
    if let Err(err) = listen(callback) {
        eprintln!("Error while listening for keyboard events: {:?}", err);
    }
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

fn replace_word(keys: Vec<Key>, terminating_key: Key) {
    use std::thread;
    use std::time::Duration;

    // Small delay to ensure the terminating key has been processed by the OS
    thread::sleep(Duration::from_millis(50));

    // Delete terminating key + word characters
    let delete_count = keys.len() + 1;
    for _ in 0..delete_count {
        let _ = simulate(&EventType::KeyPress(Key::Backspace));
        let _ = simulate(&EventType::KeyRelease(Key::Backspace));
        thread::sleep(Duration::from_millis(1));
    }

    // Small delay to allow the OS to process layout switch
    thread::sleep(Duration::from_millis(30));

    // Retype the original physical keys
    for key in keys {
        let _ = simulate(&EventType::KeyPress(key));
        let _ = simulate(&EventType::KeyRelease(key));
        thread::sleep(Duration::from_millis(1));
    }

    // Retype the terminating key
    let _ = simulate(&EventType::KeyPress(terminating_key));
    let _ = simulate(&EventType::KeyRelease(terminating_key));
}

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
        }

        let current_src = TISCopyCurrentKeyboardInputSource();
        let mut switched = false;

        if current_src.is_null() || core_foundation_sys::base::CFEqual(src as CFTypeRef, current_src as CFTypeRef) == 0 {
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

fn key_to_english_char(key: Key) -> Option<char> {
    match key {
        Key::KeyA => Some('a'),
        Key::KeyB => Some('b'),
        Key::KeyC => Some('c'),
        Key::KeyD => Some('d'),
        Key::KeyE => Some('e'),
        Key::KeyF => Some('f'),
        Key::KeyG => Some('g'),
        Key::KeyH => Some('h'),
        Key::KeyI => Some('i'),
        Key::KeyJ => Some('j'),
        Key::KeyK => Some('k'),
        Key::KeyL => Some('l'),
        Key::KeyM => Some('m'),
        Key::KeyN => Some('n'),
        Key::KeyO => Some('o'),
        Key::KeyP => Some('p'),
        Key::KeyQ => Some('q'),
        Key::KeyR => Some('r'),
        Key::KeyS => Some('s'),
        Key::KeyT => Some('t'),
        Key::KeyU => Some('u'),
        Key::KeyV => Some('v'),
        Key::KeyW => Some('w'),
        Key::KeyX => Some('x'),
        Key::KeyY => Some('y'),
        Key::KeyZ => Some('z'),
        Key::Num1 => Some('1'),
        Key::Num2 => Some('2'),
        Key::Num3 => Some('3'),
        Key::Num4 => Some('4'),
        Key::Num5 => Some('5'),
        Key::Num6 => Some('6'),
        Key::Num7 => Some('7'),
        Key::Num8 => Some('8'),
        Key::Num9 => Some('9'),
        Key::Num0 => Some('0'),
        _ => None,
    }
}

fn key_to_hebrew_char(key: Key) -> Option<char> {
    // Rough mapping for standard Hebrew layout on QWERTY.
    match key {
        Key::KeyQ => Some('ק'),
        Key::KeyW => Some('ו'),
        Key::KeyE => Some('ק'),
        Key::KeyR => Some('ר'),
        Key::KeyT => Some('ת'),
        Key::KeyY => Some('י'),
        Key::KeyU => Some('ו'),
        Key::KeyI => Some('ט'),
        Key::KeyO => Some('ם'),
        Key::KeyP => Some('פ'),
        Key::KeyA => Some('ש'),
        Key::KeyS => Some('ד'),
        Key::KeyD => Some('ג'),
        Key::KeyF => Some('כ'),
        Key::KeyG => Some('ע'),
        Key::KeyH => Some('י'),
        Key::KeyJ => Some('ח'),
        Key::KeyK => Some('ל'),
        Key::KeyL => Some('ך'),
        Key::KeyZ => Some('ז'),
        Key::KeyX => Some('ס'),
        Key::KeyC => Some('ב'),
        Key::KeyV => Some('ה'),
        Key::KeyB => Some('נ'),
        Key::KeyN => Some('מ'),
        Key::KeyM => Some('צ'),
        _ => None,
    }
}

