use rdev::{listen, Event, EventType, Key};
#[cfg(target_os = "linux")]
use evdev::{Device, InputEventKind, Key as EvKey};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
use std::process::Command;

#[cfg(target_os = "macos")]
use core_foundation::base::TCFType;
#[cfg(target_os = "macos")]
use core_foundation::string::CFString;
#[cfg(target_os = "macos")]
use core_foundation_sys::base::CFTypeRef;
#[cfg(target_os = "macos")]
use core_foundation_sys::string::CFStringRef;

// FFI bindings to macOS Text Input Source (TIS) APIs from the Carbon framework.
#[cfg(target_os = "macos")]
#[repr(C)]
struct __TISInputSource;
#[cfg(target_os = "macos")]
type TISInputSourceRef = *mut __TISInputSource;

#[cfg(target_os = "macos")]
#[link(name = "Carbon")]
extern "C" {
    fn TISCopyInputSourceForLanguage(language: CFStringRef) -> TISInputSourceRef;
    fn TISSelectInputSource(source: TISInputSourceRef) -> i32; // OSStatus
    fn CFRelease(cf: CFTypeRef);
}

#[derive(Clone, Copy, Debug)]
enum Language {
    English,
    Hebrew,
}

fn main() {
    println!("Starting typeLan keyboard watcher...");

    // --- Configuration ---
    let en_dict_path = "en_dict.txt";
    let he_dict_path = "he_dict.txt";

    // --- Load Dictionaries ---
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

    // --- Shared State ---
    // We build two parallel candidate words based on the physical keys:
    // - one as if typed on an English layout
    // - one as if typed on a Hebrew layout
    let word_en = Arc::new(Mutex::new(String::new()));
    let word_he = Arc::new(Mutex::new(String::new()));

    #[cfg(target_os = "macos")]
    {
        // Clone for the callback closure
        let en_dict_cb = en_dict.clone();
        let he_dict_cb = he_dict.clone();
        let word_en_cb = Arc::clone(&word_en);
        let word_he_cb = Arc::clone(&word_he);

        let callback = move |event: Event| {
            let mut word_en = word_en_cb.lock().unwrap();
            let mut word_he = word_he_cb.lock().unwrap();

            match event.event_type {
                EventType::KeyPress(key) => match key {
                    Key::Space | Key::Return => {
                        if !word_en.is_empty() || !word_he.is_empty() {
                            println!(
                                "Word finished. EN candidate: '{}', HE candidate: '{}'",
                                *word_en, *word_he
                            );
                            check_and_switch_candidates(
                                &word_en,
                                &word_he,
                                &en_dict_cb,
                                &he_dict_cb,
                            );
                            word_en.clear();
                            word_he.clear();
                        }
                    }
                    Key::Backspace => {
                        word_en.pop();
                        word_he.pop();
                    }
                    _ => {
                        if let Some(ch) = key_to_english_char(key) {
                            word_en.push(ch);
                        }
                        if let Some(ch) = key_to_hebrew_char(key) {
                            word_he.push(ch);
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

    #[cfg(target_os = "linux")]
    {
        // Capture Hyprland instance signature (if available) for use when running under sudo.
        if let Err(err) = capture_hypr_signature() {
            eprintln!("Warning: could not capture Hyprland instance signature: {:?}", err);
        }

        println!("Listening for keyboard events via evdev. Press Space or Enter to check a word.");
        if let Err(err) = run_linux_evdev_listener(word_en, word_he, en_dict, he_dict) {
            eprintln!("Error while listening for keyboard events via evdev: {:?}", err);
        }
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
) {
    let word_en_lower = word_en.to_lowercase();
    let word_he_lower = word_he.to_lowercase();
    let is_in_en = !word_en_lower.is_empty() && en_dict.contains(&word_en_lower);
    let is_in_he = !word_he_lower.is_empty() && he_dict.contains(&word_he_lower);

    println!(
        "Checking candidates. EN='{}' (in EN: {}), HE='{}' (in HE: {})",
        word_en, is_in_en, word_he, is_in_he
    );

    if is_in_en && !is_in_he {
        println!("Recognized as English word. Selecting English layout.");
        switch_layout_to(Language::English);
    } else if is_in_he && !is_in_en {
        println!("Recognized as Hebrew word. Selecting Hebrew layout.");
        switch_layout_to(Language::Hebrew);
    }
}

#[cfg(target_os = "macos")]
fn switch_layout_to(lang: Language) {
    let code = match lang {
        Language::English => "en",
        Language::Hebrew => "he",
    };

    unsafe {
        let cf_lang = CFString::new(code);
        let src = TISCopyInputSourceForLanguage(cf_lang.as_concrete_TypeRef());

        if src.is_null() {
            eprintln!("No input source found for language code '{}'", code);
            return;
        }

        let status = TISSelectInputSource(src);
        if status != 0 {
            eprintln!(
                "TISSelectInputSource failed for '{}' with status {}",
                code, status
            );
        } else {
            println!("Switched macOS input source to '{}'", code);
        }

        CFRelease(src as CFTypeRef);
    }
}

#[cfg(target_os = "linux")]
fn switch_layout_to(lang: Language) {
    // This assumes your Hyprland config sets:
    //   kb_layout = us,il
    // so index 0 = English (us), index 1 = Hebrew (il).
    let index = match lang {
        Language::English => 0_i32,
        Language::Hebrew => 1_i32,
    };

    println!(
        "Attempting to switch Hyprland keyboard layout via hyprctl switchxkblayout current {}",
        index
    );

    let mut cmd = Command::new("hyprctl");
    cmd.arg("switchxkblayout")
        .arg("current")
        .arg(index.to_string());

    // When running under sudo, Hyprland env vars may not be set.
    // If we have previously captured them, inject them into the environment.
    if let Ok(env) = std::fs::read_to_string(".hypr_env") {
        for line in env.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                let val = v.trim();
                if !key.is_empty() && !val.is_empty() {
                    cmd.env(key, val);
                }
            }
        }
    }

    match cmd.status() {
        Ok(status) if status.success() => {
            println!(
                "Switched Hyprland keyboard layout to index {} ({}).",
                index,
                match lang {
                    Language::English => "English/us",
                    Language::Hebrew => "Hebrew/il",
                }
            );
        }
        Ok(status) => {
            eprintln!(
                "hyprctl exited with status code {:?} while switching xkb layout to index {}",
                status.code(),
                index
            );
        }
        Err(err) => {
            eprintln!(
                "Failed to execute hyprctl switchxkblayout current {}: {}. \
Ensure hyprctl is installed and kb_layout is configured as 'us,il'.",
                index,
                err
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn capture_hypr_signature() -> io::Result<()> {
    let mut lines = Vec::new();

    if let Ok(sig) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        lines.push(format!("HYPRLAND_INSTANCE_SIGNATURE={}", sig));
    }

    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        lines.push(format!("XDG_RUNTIME_DIR={}", runtime));
    }

    if !lines.is_empty() {
        std::fs::write(".hypr_env", lines.join("\n"))?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn run_linux_evdev_listener(
    word_en: Arc<Mutex<String>>,
    word_he: Arc<Mutex<String>>,
    en_dict: HashSet<String>,
    he_dict: HashSet<String>,
) -> io::Result<()> {
    use std::fs;

    let mut devices = Vec::new();
    for entry in fs::read_dir("/dev/input")? {
        let entry = entry?;
        let path = entry.path();
        if !path.to_string_lossy().contains("event") {
            continue;
        }

        if let Ok(dev) = Device::open(&path) {
            if dev.supported_keys().map_or(false, |keys| {
                keys.contains(EvKey::KEY_A) || keys.contains(EvKey::KEY_SPACE)
            }) {
                println!("Using input device: {}", path.display());
                devices.push(dev);
            }
        }
    }

    if devices.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No suitable keyboard input devices found under /dev/input",
        ));
    }

    loop {
        for dev in devices.iter_mut() {
            for ev in dev.fetch_events()? {
                if let InputEventKind::Key(key) = ev.kind() {
                    // value: 1 = key press, 0 = release, 2 = repeat
                    if ev.value() != 1 {
                        continue;
                    }

                    let mut word_en_lock = word_en.lock().unwrap();
                    let mut word_he_lock = word_he.lock().unwrap();

                    match key {
                        EvKey::KEY_SPACE | EvKey::KEY_ENTER => {
                            if !word_en_lock.is_empty() || !word_he_lock.is_empty() {
                                println!(
                                    "Word finished. EN candidate: '{}', HE candidate: '{}'",
                                    *word_en_lock, *word_he_lock
                                );
                                check_and_switch_candidates(
                                    &word_en_lock,
                                    &word_he_lock,
                                    &en_dict,
                                    &he_dict,
                                );
                                word_en_lock.clear();
                                word_he_lock.clear();
                            }
                        }
                        EvKey::KEY_BACKSPACE => {
                            word_en_lock.pop();
                            word_he_lock.pop();
                        }
                        _ => {
                            if let Some(ch) = evkey_to_english_char(key) {
                                word_en_lock.push(ch);
                            }
                            if let Some(ch) = evkey_to_hebrew_char(key) {
                                word_he_lock.push(ch);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn evkey_to_english_char(key: EvKey) -> Option<char> {
    match key {
        EvKey::KEY_A => Some('a'),
        EvKey::KEY_B => Some('b'),
        EvKey::KEY_C => Some('c'),
        EvKey::KEY_D => Some('d'),
        EvKey::KEY_E => Some('e'),
        EvKey::KEY_F => Some('f'),
        EvKey::KEY_G => Some('g'),
        EvKey::KEY_H => Some('h'),
        EvKey::KEY_I => Some('i'),
        EvKey::KEY_J => Some('j'),
        EvKey::KEY_K => Some('k'),
        EvKey::KEY_L => Some('l'),
        EvKey::KEY_M => Some('m'),
        EvKey::KEY_N => Some('n'),
        EvKey::KEY_O => Some('o'),
        EvKey::KEY_P => Some('p'),
        EvKey::KEY_Q => Some('q'),
        EvKey::KEY_R => Some('r'),
        EvKey::KEY_S => Some('s'),
        EvKey::KEY_T => Some('t'),
        EvKey::KEY_U => Some('u'),
        EvKey::KEY_V => Some('v'),
        EvKey::KEY_W => Some('w'),
        EvKey::KEY_X => Some('x'),
        EvKey::KEY_Y => Some('y'),
        EvKey::KEY_Z => Some('z'),
        EvKey::KEY_1 => Some('1'),
        EvKey::KEY_2 => Some('2'),
        EvKey::KEY_3 => Some('3'),
        EvKey::KEY_4 => Some('4'),
        EvKey::KEY_5 => Some('5'),
        EvKey::KEY_6 => Some('6'),
        EvKey::KEY_7 => Some('7'),
        EvKey::KEY_8 => Some('8'),
        EvKey::KEY_9 => Some('9'),
        EvKey::KEY_0 => Some('0'),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn evkey_to_hebrew_char(key: EvKey) -> Option<char> {
    match key {
        EvKey::KEY_Q => Some('ק'),
        EvKey::KEY_W => Some('ו'),
        EvKey::KEY_E => Some('ק'), // placeholder; adjust as needed
        EvKey::KEY_R => Some('ר'),
        EvKey::KEY_T => Some('ת'),
        EvKey::KEY_Y => Some('י'),
        // Adjusted mapping so that the physical key sequence "akuo"
        // corresponds to the Hebrew word "שלום".
        EvKey::KEY_U => Some('ו'),
        EvKey::KEY_I => Some('ט'),
        EvKey::KEY_O => Some('ם'),
        EvKey::KEY_P => Some('פ'),
        EvKey::KEY_A => Some('ש'),
        EvKey::KEY_S => Some('ד'),
        EvKey::KEY_D => Some('ג'),
        EvKey::KEY_F => Some('כ'),
        EvKey::KEY_G => Some('ע'),
        EvKey::KEY_H => Some('י'),
        EvKey::KEY_J => Some('ח'),
        EvKey::KEY_K => Some('ל'),
        EvKey::KEY_L => Some('ך'),
        EvKey::KEY_Z => Some('ז'),
        EvKey::KEY_X => Some('ס'),
        EvKey::KEY_C => Some('ב'),
        EvKey::KEY_V => Some('ה'),
        EvKey::KEY_B => Some('נ'),
        EvKey::KEY_N => Some('מ'),
        EvKey::KEY_M => Some('צ'),
        _ => None,
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
        Key::KeyE => Some('ק'), // placeholder; adjust as needed
        Key::KeyR => Some('ר'),
        Key::KeyT => Some('ת'),
        Key::KeyY => Some('י'),
        // Adjusted mapping so that the physical key sequence "akuo"
        // corresponds to the Hebrew word "שלום".
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

