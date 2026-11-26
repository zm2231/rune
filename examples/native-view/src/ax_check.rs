#![allow(unsafe_code)]

use std::ptr::NonNull;
use std::time::Duration;

use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement};
use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType};

pub fn start() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(1));
        println!("native-view: ax {}", read());
    });
}

fn read() -> String {
    // SAFETY: AXIsProcessTrusted takes no arguments and only reads this process's trust.
    if !unsafe { AXIsProcessTrusted() } { return "untrusted: allow this terminal in Privacy & Security > Accessibility".into(); }
    // SAFETY: the pid is this process's own, which is alive for the call.
    let app = unsafe { AXUIElement::new_application(std::process::id() as libc::pid_t) };
    // SAFETY: `app` is a valid element.
    let _ = unsafe { app.set_messaging_timeout(1.0) };
    let mut stack = children(&app, "AXWindows");
    let mut seen = 0;
    while let Some(element) = stack.pop() {
        seen += 1;
        if seen > 5000 { break; }
        for name in ["AXValue", "AXTitle", "AXDescription"] {
            if let Some(text) = attribute(&element, name).and_then(|v| v.downcast::<CFString>().ok()).map(|s| s.to_string())
                && (text.starts_with("Up for") || text.starts_with("Page up") || text.starts_with("No page"))
            {
                return text;
            }
        }
        stack.extend(children(&element, "AXChildren"));
    }
    format!("no label among {seen} elements")
}

fn children(element: &AXUIElement, name: &str) -> Vec<CFRetained<AXUIElement>> {
    let Some(array) = attribute(element, name).and_then(|v| v.downcast::<CFArray>().ok()) else { return Vec::new() };
    // SAFETY: every element of a CFArray is a CoreFoundation object.
    let array = unsafe { array.cast_unchecked::<CFType>() };
    array.iter().filter_map(|v| v.downcast::<AXUIElement>().ok()).collect()
}

fn attribute(element: &AXUIElement, name: &str) -> Option<CFRetained<CFType>> {
    let mut value: *const CFType = std::ptr::null();
    // SAFETY: `value` is a valid out-pointer. On success it holds a +1 reference to a
    // CoreFoundation object, which `CFRetained::from_raw` takes over.
    unsafe {
        if element.copy_attribute_value(&CFString::from_str(name), NonNull::from(&mut value)) != AXError::Success { return None; }
        NonNull::new(value.cast_mut()).map(|v| CFRetained::from_raw(v))
    }
}
