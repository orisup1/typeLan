mod dictionary;
#[cfg(target_os = "linux")]
mod gui;
mod keymap;
mod layout;
mod platform;
mod types;

use std::collections::HashSet;
use std::sync::Arc;

use dictionary::load_dictionary;
use types::AppControl;

fn main() {
    let with_gui = std::env::args().skip(1).any(|a| a == "-g" || a == "--gui");

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

    let control = Arc::new(AppControl::new());

    #[cfg(target_os = "linux")]
    {
        if with_gui {
            // Keyboard listener runs in a background thread; the GUI must own
            // the main thread because eframe drives the windowing event loop.
            let listener_control = Arc::clone(&control);
            std::thread::spawn(move || {
                platform::linux::run(en_dict, he_dict, listener_control);
            });
            if let Err(e) = gui::run(control) {
                eprintln!("GUI error: {}", e);
            }
            return;
        }
        platform::linux::run(en_dict, he_dict, control);
    }

    #[cfg(target_os = "macos")]
    {
        let _ = with_gui; // GUI flag is currently Linux-only.
        platform::macos::run(en_dict, he_dict, control);
    }

    #[cfg(target_os = "windows")]
    {
        let _ = with_gui;
        platform::windows::run(en_dict, he_dict, control);
    }
}
