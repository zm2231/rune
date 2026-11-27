//! Main-thread macOS checks for `rune::Tray`, without opening a window or touching the live
//! menu bar by default: the tray's AppKit menus, images and clicks run against a fake status
//! item. Set `RUNE_GUI_TESTS=1` in a logged-in macOS GUI session to also exercise creating,
//! updating and removing a real `NSStatusItem`.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    use objc2::MainThreadMarker;
    use rune::main_thread_checks::tray as checks;
    use rune::{Tray, TrayError, TrayIcon, TrayItem, TraySpec};

    let mtm = MainThreadMarker::new().expect("a harness-free test runs on the main thread");

    // Zero dimensions fail cleanly on the main thread before creating any NSStatusItem.
    for (w, h) in [(0, 0), (0, 16), (16, 0)] {
        let spec = TraySpec::new("dev.rune.test", "Test", TrayIcon::rgba(w, h, Vec::new()));
        assert!(matches!(Tray::new(spec, |_| {}), Err(TrayError::Backend(_))));
    }

    // Mismatched pixel buffers fail before touching AppKit.
    let bad = TraySpec::new("dev.rune.test", "Test", TrayIcon::rgba(4, 4, vec![0; 8]));
    assert!(matches!(Tray::new(bad, |_| {}), Err(TrayError::Backend(_))));

    // Creating a tray off the main thread returns TrayError::Backend.
    let off_main = std::thread::spawn(|| {
        let spec = TraySpec::new(
            "dev.rune.test",
            "Test",
            TrayIcon::rgba(16, 16, vec![255; 16 * 16 * 4]).template(true),
        );
        Tray::new(spec, |_| {})
    })
    .join()
    .unwrap();
    assert!(matches!(off_main, Err(TrayError::Backend(_))));

    checks::fake_host_lifecycle_menu_tooltip_clicks_and_drop(mtm);
    checks::callback_panic_is_caught_and_reentrant_menu_click_is_ignored(mtm);
    checks::fake_host_set_icon_set_title_rect_and_check_menu_items(mtm);
    checks::set_highlighted_survives_icon_title_and_menu_updates(mtm);
    checks::menu_on_primary_click_attaches_a_menu_filled_when_it_opens(mtm);
    println!("tray_macos: fake status item checks ok");

    if std::env::var_os("RUNE_GUI_TESTS").is_none() {
        println!("tray_macos: live NSStatusBar check skipped (set RUNE_GUI_TESTS=1 in a GUI session)");
        return;
    }

    let spec = TraySpec::new(
        "dev.rune.test",
        "Rune Tray Test",
        TrayIcon::rgba(32, 32, vec![255; 32 * 32 * 4]).template(true),
    )
    .tooltip("Rune Tray Test")
    .menu(vec![
        TrayItem::action("open", "Open"),
        TrayItem::Separator,
        TrayItem::action("quit", "Quit"),
    ]);
    let tray = Tray::new(spec, |_| {}).expect("create macOS tray");
    tray.set_tooltip("Updated tooltip");
    tray.set_title("42%");
    tray.set_icon(TrayIcon::rgba(32, 32, vec![255, 64, 64, 255].repeat(32 * 32))).expect("update icon");
    let _ = tray.rect();
    tray.set_title("");
    tray.set_menu(vec![
        TrayItem::check("hud", "Show HUD", true),
        TrayItem::action("quit", "Quit").enabled(false),
    ]);
    drop(tray);
}
