use super::*;
use objc2_foundation::NSPoint;

#[test]
fn make_image_sets_point_size_template_accessibility_and_premultiplied_pixels() {
    // 44x44 @2x template icon fits 22x22 pt menu bar.
    let icon = TrayIcon::rgba(44, 44, vec![255; 44 * 44 * 4]).template(true);
    let image = make_image(&icon, 22.0, "Rune Status").unwrap();
    assert_eq!(image.size(), NSSize::new(22.0, 22.0));
    assert!(image.isTemplate());
    assert_eq!(image.accessibilityDescription().map(|s| s.to_string()), Some("Rune Status".into()));
    let reps = image.representations();
    assert_eq!(reps.len(), 1);
    let rep = reps.objectAtIndex(0);
    assert_eq!(rep.pixelsWide(), 44);
    assert_eq!(rep.pixelsHigh(), 44);
    assert_eq!(rep.size(), NSSize::new(22.0, 22.0));

    // Wide 4x2 non-template icon preserves 2:1 aspect ratio -> 44x22 pt and premultiplies alpha.
    let mut pixels = vec![0u8; 4 * 2 * 4];
    // Pixel (0, 0): straight RGBA (200, 100, 50, 128) -> premultiplied (100, 50, 25, 128).
    pixels[0..4].copy_from_slice(&[200, 100, 50, 128]);
    // Pixel (1, 0): fully transparent straight RGBA (255, 255, 255, 0) -> (0, 0, 0, 0).
    pixels[4..8].copy_from_slice(&[255, 255, 255, 0]);
    let wide = TrayIcon::rgba(4, 2, pixels);
    let wide_img = make_image(&wide, 22.0, "").unwrap();
    assert_eq!(wide_img.size(), NSSize::new(44.0, 22.0));
    assert!(!wide_img.isTemplate());
    assert!(wide_img.accessibilityDescription().is_none());
    let rep = wide_img.representations().objectAtIndex(0);
    let bitmap = rep.downcast::<NSBitmapImageRep>().expect("NSBitmapImageRep");
    // SAFETY: bitmap owns 4x2x4 bytes with stride >= 16.
    let first_row = unsafe { std::slice::from_raw_parts(bitmap.bitmapData(), 8) };
    assert_eq!(&first_row[0..4], &[100, 50, 25, 128]);
    assert_eq!(&first_row[4..8], &[0, 0, 0, 0]);
}

#[test]
fn make_image_refuses_zero_or_mismatched_dimensions() {
    for (w, h, len) in [(0, 0, 0), (0, 16, 0), (16, 0, 0), (2, 2, 12)] {
        let icon = TrayIcon::rgba(w, h, vec![0; len]);
        assert!(matches!(make_image(&icon, 22.0, "T"), Err(TrayError::Backend(_))));
    }
}

#[test]
fn tray_new_off_main_thread_returns_backend_error() {
    let result = std::thread::spawn(|| {
        let spec = TraySpec::new("id", "Title", TrayIcon::rgba(16, 16, vec![255; 16 * 16 * 4]));
        super::super::Tray::new(spec, |_| {})
    })
    .join()
    .unwrap();
    assert!(matches!(result, Err(TrayError::Backend(_))));
}

#[test]
fn event_click_classifies_synthetic_nsevent_clicks() {
    let mk = |ty, flags| {
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            ty,
            NSPoint::new(0.0, 0.0),
            flags,
            0.0,
            0,
            None,
            0,
            1,
            1.0,
        )
        .unwrap()
    };
    assert_eq!(event_click(None), TrayClick::Primary);
    assert_eq!(event_click(Some(&mk(NSEventType::LeftMouseUp, NSEventModifierFlags::empty()))), TrayClick::Primary);
    assert_eq!(event_click(Some(&mk(NSEventType::RightMouseUp, NSEventModifierFlags::empty()))), TrayClick::Secondary);
    assert_eq!(event_click(Some(&mk(NSEventType::LeftMouseUp, NSEventModifierFlags::Control))), TrayClick::Secondary);
}
