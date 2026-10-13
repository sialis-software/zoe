//! Platform touches that iced does not offer.
//!
//! On macOS a settings window is an accessory of the one it belongs to, like an inspector: it
//! travels with it, stays above it, and shows over it in a full screen space. That takes the
//! native window; everywhere else the settings are an ordinary small window.

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

/// The menu bar's Settings… item being chosen.
pub fn settings_chosen() -> Subscription<()> {
    imp::settings_chosen()
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

    use iced::futures::StreamExt;
    use iced::futures::channel::mpsc;
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use std::sync::Mutex;

    const SETTINGS: &str = "settings";
    static CHOSEN: Mutex<Option<mpsc::UnboundedSender<()>>> = Mutex::new(None);

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
            let settings = MenuItem::with_id(SETTINGS, "Settings\u{2026}", true, Some(Accelerator::new(Modifiers::META, Code::Comma)));
            let _ = app.append_items(&[
                &PredefinedMenuItem::about(None, None),
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

    pub fn settings_chosen() -> Subscription<()> {
        Subscription::run(|| {
            let (tx, rx) = mpsc::unbounded();
            *CHOSEN.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
            MenuEvent::set_event_handler(Some(|event: MenuEvent| {
                if event.id == SETTINGS
                    && let Some(tx) = CHOSEN.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                {
                    let _ = tx.unbounded_send(());
                }
            }));
            rx.map(|()| ())
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

    pub fn settings_chosen() -> Subscription<()> {
        Subscription::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_a_square_picture_with_clear_corners() {
        let (pixels, width, height) = icon_pixels().expect("an 8-bit RGBA picture");
        assert_eq!((width, height), (1024, 1024));
        let alpha = |x: usize, y: usize| pixels[(y * width as usize + x) * 4 + 3];
        assert_eq!((alpha(0, 0), alpha(1023, 1023)), (0, 0), "rounded: the corners show through");
        assert_eq!(alpha(512, 512), 255);
    }
}
