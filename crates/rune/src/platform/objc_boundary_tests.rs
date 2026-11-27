use super::guarded;
use std::process::{Command, Output};

const CHILD: &str = "RUNE_OBJC_BOUNDARY_CHILD";

fn in_child(test: &str) -> Output {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([&format!("platform::objc_boundary::tests::{test}"), "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the process went on after the panic:\n{stderr}");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(6), "the process didn't abort:\n{stderr}");
    }
    assert!(!stderr.contains("test result"), "the test harness kept running:\n{stderr}");
    output
}

#[test]
fn a_value_passes_through() {
    assert_eq!(guarded("layerClass", || 7), 7);
}

#[test]
fn a_panic_caught_inside_the_entry_point_lets_it_return() {
    let value = guarded("runeTick:", || std::panic::catch_unwind(|| -> u8 { panic!("a view failed to build") }).unwrap_or(3));
    assert_eq!(value, 3);
    assert_eq!(guarded("layerClass", || 7), 7);
}

#[test]
fn a_panic_is_reported_with_its_entry_and_location_and_ends_the_process() {
    if std::env::var_os(CHILD).is_some() {
        guarded("runeTick:", || -> () { panic!("the tick broke") });
        return;
    }
    let output = in_child("a_panic_is_reported_with_its_entry_and_location_and_ends_the_process");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rune: a panic inside `runeTick:` at crates/rune/src/platform/objc_boundary_tests.rs:"), "{stderr}");
    assert!(stderr.contains("the tick broke"), "{stderr}");
    assert!(stderr.contains("rune: ending the app"), "{stderr}");
}

#[test]
fn the_report_comes_first_when_the_apps_own_panic_hook_panics() {
    if std::env::var_os(CHILD).is_some() {
        std::panic::set_hook(Box::new(|_| panic!("the app's hook broke")));
        guarded("touchesBegan:withEvent:", || -> () { panic!("the touch broke") });
        return;
    }
    let output = in_child("the_report_comes_first_when_the_apps_own_panic_hook_panics");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rune: a panic inside `touchesBegan:withEvent:` at"), "{stderr}");
    assert!(stderr.contains("the touch broke"), "{stderr}");
}
