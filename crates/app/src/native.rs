//! Platform touches that iced does not offer.
//!
//! On macOS a settings window is an accessory of the one it belongs to, like an inspector: it
//! travels with it, stays above it, and shows over it in a full screen space. That takes the
//! native window; everywhere else the settings are an ordinary small window.

use crate::commands::{Command, MenuState};
use iced::window::Id;
use iced::{Subscription, Task};

/// Make window `child` an accessory of window `parent` (macOS; elsewhere nothing happens).
pub fn attach(parent: Id, child: Id) -> Task<()> {
    imp::attach(parent, child)
}

/// Put the app's own menu bar in place (macOS: the application menu gets Settings…, ⌘,).
/// Wait until the window exists: the system makes its own menu as the app starts.
pub fn install_menu(window: Id) -> Task<()> {
    imp::install_menu(window)
}

/// The app's picture, as a window's icon (where windows have one: not on a Mac, whose Dock icon
/// is set from it when the app runs outside a bundle).
pub fn window_icon() -> Option<iced::window::Icon> {
    if cfg!(target_os = "macos") {
        return None;
    }
    let (pixels, width, height) = icon_pixels()?;
    iced::window::icon::from_rgba(pixels, width, height).ok()
}

/// The artwork as 8-bit RGBA, with its width and height.
fn icon_pixels() -> Option<(Vec<u8>, u32, u32)> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(ICON)).read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    (info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight).then_some((pixels, info.width, info.height))
}

/// The artwork on the rounded square (made by `scripts/make-icon.swift`) that every system gets: a
/// Dock or taskbar shows what it is given as it is.
const ICON: &[u8] = include_bytes!("../assets/app/zoe-rounded.png");

/// A menu bar item being chosen.
pub fn commands_chosen() -> Subscription<Command> {
    imp::commands_chosen()
}

/// Grey out the menu items that do nothing just now and tick the ones that are on.
pub fn sync_menu(window: Id, state: MenuState) -> Task<()> {
    imp::sync_menu(window, state)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use iced::window::{self, Window};
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindow, NSWindowCollectionBehavior, NSWindowOrderingMode};
    use raw_window_handle::RawWindowHandle;

    /// The `NSWindow` behind an iced window.
    fn native(window: &dyn Window) -> Option<Retained<NSWindow>> {
        let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else { return None };
        // SAFETY: winit keeps the view alive as long as its window, and this runs on the main
        // thread with the window, inside iced's callback.
        let view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        view.window()
    }

    use iced::futures::channel::mpsc;
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use crate::commands::Shortcut;
    use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use std::cell::RefCell;
    use std::sync::Mutex;

    static CHOSEN: Mutex<Option<mpsc::UnboundedSender<Command>>> = Mutex::new(None);

    /// A menu item kept to be changed later.
    enum Item {
        Plain(MenuItem),
        Check(CheckMenuItem),
    }

    thread_local! {
        /// The items, on the main thread (they belong to it).
        static ITEMS: RefCell<Vec<(Command, Item)>> = const { RefCell::new(Vec::new()) };
    }

    fn accelerator(shortcut: Shortcut) -> Option<Accelerator> {
        let code = match shortcut.key {
            't' => Code::KeyT,
            'n' => Code::KeyN,
            'w' => Code::KeyW,
            'f' => Code::KeyF,
            '[' => Code::BracketLeft,
            ']' => Code::BracketRight,
            ',' => Code::Comma,
            's' => Code::KeyS,
            'm' => Code::KeyM,
            'r' => Code::KeyR,
            _ => return None,
        };
        let mut mods = Modifiers::META;
        if shortcut.shift {
            mods |= Modifiers::SHIFT;
        }
        if shortcut.alt {
            mods |= Modifiers::ALT;
        }
        Some(Accelerator::new(mods, code))
    }

    /// What the system's About panel shows: the name, version and credits, with the app's picture.
    /// (In a bundle the panel would also read them from the bundle: this makes it the same outside one.)
    fn about_metadata() -> muda::AboutMetadata {
        muda::AboutMetadata {
            name: Some("Zoe".into()),
            version: Some(crate::about::version().into()),
            comments: Some("Bible study with SWORD modules and Markdown notes".into()),
            credits: Some(crate::about::credits_text()),
            icon: icon_pixels().and_then(|(rgba, width, height)| muda::Icon::from_rgba(rgba, width, height).ok()),
            ..muda::AboutMetadata::default()
        }
    }

    pub fn install_menu(window: Id) -> Task<()> {
        window::run(window, |_| {
            // Run outside an app bundle, the system would name the app after its file: Zoe is its name.
            objc2_foundation::NSProcessInfo::processInfo().setProcessName(&objc2_foundation::NSString::from_str("Zoe"));
            // Likewise the Dock's picture: a bundle has its own, and this only matters without one.
            if let Some(mtm) = objc2::MainThreadMarker::new() {
                let data = objc2_foundation::NSData::with_bytes(ICON);
                if let Some(image) = objc2_app_kit::NSImage::initWithData(<objc2_app_kit::NSImage as objc2::AnyThread>::alloc(), &data) {
                    unsafe { objc2_app_kit::NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
                }
            }
            let menu = Menu::new();
            let app = Submenu::new("Zoe", true);
            let mut kept: Vec<(Command, Item)> = Vec::new();
            let mut item = |command: Command| -> MenuItem {
                let made = MenuItem::with_id(command.id(), command.label(), true, command.shortcut().and_then(accelerator));
                kept.push((command, Item::Plain(made.clone())));
                made
            };
            let settings = item(Command::Settings);
            let (new_tab, new_pane, close_tab) = (item(Command::NewTab), item(Command::NewPane), item(Command::CloseTab));
            let (back, forward) = (item(Command::Back), item(Command::Forward));
            let (previous, next, search) = (item(Command::PreviousTab), item(Command::NextTab), item(Command::Search));
            let reader = CheckMenuItem::with_id(Command::ReaderMode.id(), Command::ReaderMode.label(), true, false, None);
            kept.push((Command::ReaderMode, Item::Check(reader.clone())));
            let _ = app.append_items(&[
                &PredefinedMenuItem::about(None, Some(about_metadata())),
                &PredefinedMenuItem::separator(),
                &settings,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ]);
            let _ = menu.append(&app);
            let file = Submenu::new("File", true);
            let _ = file.append_items(&[&new_pane, &new_tab, &PredefinedMenuItem::separator(), &close_tab]);
            let view = Submenu::new("View", true);
            let mut check = |command: Command| -> CheckMenuItem {
                let made = CheckMenuItem::with_id(command.id(), command.label(), false, false, command.shortcut().and_then(accelerator));
                kept.push((command, Item::Check(made.clone())));
                made
            };
            let shown = [Command::ToggleStrongs, Command::ToggleMarks, Command::ToggleFootnotes, Command::ToggleRefs].map(&mut check);
            let margin = [Command::StrongsInMargin, Command::MarksInMargin, Command::FootnotesInMargin, Command::RefsInMargin].map(&mut check);
            let _ = view.append(&reader);
            let _ = view.append(&PredefinedMenuItem::separator());
            for made in &shown {
                let _ = view.append(made);
            }
            let _ = view.append(&PredefinedMenuItem::separator());
            for made in &margin {
                let _ = view.append(made);
            }
            let go = Submenu::new("Go", true);
            let _ = go.append_items(&[&back, &forward, &PredefinedMenuItem::separator(), &previous, &next, &PredefinedMenuItem::separator(), &search]);
            let _ = menu.append_items(&[&file, &view, &go]);
            ITEMS.with(|items| *items.borrow_mut() = kept);
            menu.init_for_nsapp();
            give_symbol("Settings", "gearshape");
            // The menu bar belongs to the app for as long as it runs.
            std::mem::forget(menu);
        })
    }

    /// Put the system symbol `symbol` on the application menu's item titled `title` (the gear
    /// beside Settings…). Without the symbol, or the item, nothing is shown.
    fn give_symbol(title: &str, symbol: &str) {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSString;
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some(app_menu) = NSApplication::sharedApplication(mtm).mainMenu().and_then(|m| m.itemAtIndex(0)).and_then(|i| i.submenu()) else {
            return;
        };
        let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None) else { return };
        for item in app_menu.itemArray() {
            if item.title().to_string().starts_with(title) {
                item.setImage(Some(&image));
            }
        }
    }

    pub fn commands_chosen() -> Subscription<Command> {
        Subscription::run(|| {
            let (tx, rx) = mpsc::unbounded();
            *CHOSEN.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
            MenuEvent::set_event_handler(Some(|event: MenuEvent| {
                if let Some(command) = Command::from_id(&event.id.0)
                    && let Some(tx) = CHOSEN.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                {
                    let _ = tx.unbounded_send(command);
                }
            }));
            rx
        })
    }

    pub fn sync_menu(window: Id, state: MenuState) -> Task<()> {
        window::run(window, move |_| {
            ITEMS.with(|items| {
                for (command, item) in items.borrow().iter() {
                    match item {
                        Item::Plain(i) => i.set_enabled(state.enabled(*command)),
                        Item::Check(i) => {
                            i.set_enabled(state.enabled(*command));
                            i.set_checked(state.checked(*command));
                        }
                    }
                }
            });
        })
    }

    pub fn attach(parent: Id, child: Id) -> Task<()> {
        // The parent's window is passed along as a pointer (the callbacks must be `Send`), and
        // is only used on the main thread, inside the second callback.
        window::run(parent, |w| native(w).map(|n| Retained::as_ptr(&n) as usize)).then(move |parent_ptr| {
            window::run(child, move |w| {
                let (Some(parent_ptr), Some(child)) = (parent_ptr, native(w)) else { return };
                // SAFETY: the parent window is alive: it was just asked for, on this thread.
                let parent = unsafe { &*(parent_ptr as *const NSWindow) };
                child.setCollectionBehavior(child.collectionBehavior() | NSWindowCollectionBehavior::FullScreenAuxiliary);
                unsafe { parent.addChildWindow_ordered(&child, NSWindowOrderingMode::Above) };
            })
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;

    pub fn attach(_parent: Id, _child: Id) -> Task<()> {
        Task::none()
    }

    pub fn install_menu(_window: Id) -> Task<()> {
        Task::none()
    }

    pub fn commands_chosen() -> Subscription<Command> {
        Subscription::none()
    }

    pub fn sync_menu(_window: Id, _state: MenuState) -> Task<()> {
        Task::none()
    }
}

/// Ask the system to open an address in the program that handles it (the browser for a web page,
/// the mail program for a mail address). Only web and mail addresses are passed on; false if it
/// was not one or the system could not be asked.
pub fn open_external(address: &str) -> bool {
    let address = address.trim();
    if crate::network::blocked() || !md_editor::is_external(address) {
        return false;
    }
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    command.arg(address).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_and_mail_addresses_are_handed_to_other_apps() {
        assert!(!open_external("file:///etc/passwd"));
        assert!(!open_external("-a Calculator"));
        assert!(!open_external("https://x.org/a b"));
        assert!(!open_external(""));
    }

    #[test]
    fn the_icon_is_a_square_picture_with_clear_corners() {
        let (pixels, width, height) = icon_pixels().expect("an 8-bit RGBA picture");
        assert_eq!((width, height), (1024, 1024));
        let alpha = |x: usize, y: usize| pixels[(y * width as usize + x) * 4 + 3];
        assert_eq!((alpha(0, 0), alpha(1023, 1023)), (0, 0), "rounded: the corners show through");
        assert_eq!(alpha(512, 512), 255);
    }
}
