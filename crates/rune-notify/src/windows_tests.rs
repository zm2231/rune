use super::*;

#[test]
fn a_silent_toast_without_a_thread_has_no_header() {
    let xml = xml(&Notification::new("n-1", "Done").body("Built"), None);
    assert!(xml.contains("<audio silent=\"true\"/>"));
    assert!(!xml.contains("<header"));
    assert!(!xml.contains("<image"));
}

#[test]
fn a_thread_is_a_header_that_clicks_like_the_toast() {
    let n = Notification::new("n-1", "Done").info("session", "s 1").thread("s 1", "Fix the build").sound(true);
    let xml = xml(&n, None);
    assert!(xml.starts_with("<toast launch=\"session=s%201\"><header id=\"s 1\" title=\"Fix the build\" arguments=\"session=s%201\"/><visual>"), "{xml}");
    assert!(xml.ends_with("<audio src=\"ms-winsoundevent:Notification.Default\"/></toast>"), "{xml}");
    let xml = super::xml(&n, Some("demo"));
    assert!(xml.contains("<header id=\"s 1\" title=\"Fix the build\" arguments=\"demo://notification/n-1?session=s%201\" activationType=\"protocol\"/>"), "{xml}");
}

#[test]
fn the_first_image_shows_inline() {
    let dir = std::env::temp_dir().join(format!("rune-notify-windows-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("shot.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
    let n = Notification::new("n-1", "Done").attachment(crate::Attachment::file(&png).unwrap());
    let xml = xml(&n, None);
    std::fs::remove_dir_all(&dir).unwrap();
    let uri = crate::attachment::file_uri(&std::path::absolute(&png).unwrap());
    assert!(xml.contains(&format!("<text>Done</text><image src=\"{uri}\"/></binding>")), "{xml}");
}
