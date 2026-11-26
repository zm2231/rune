//! Rune's native menu bar on the main thread: nested submenus build, fire through AppKit and are
//! cleared on teardown, without opening a window or running an event loop.

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    let mtm = objc2::MainThreadMarker::new().expect("a harness-free test runs on the main thread");
    rune::main_thread_checks::native_menu::macos_native_submenus_build_fire_and_clear(mtm);
    println!("native_menu_macos: ok");
}
