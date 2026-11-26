use super::*;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("rune-notify-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[test]
fn the_format_comes_from_the_first_bytes() {
    let cases: &[(&[u8], Option<Format>)] = &[
        (PNG, Some(Format::Png)),
        (&[0xff, 0xd8, 0xff, 0xe0], Some(Format::Jpeg)),
        (b"GIF89a", Some(Format::Gif)),
        (b"FORM\0\0\0\0AIFF", Some(Format::Aiff)),
        (b"RIFF\0\0\0\0WAVE", Some(Format::Wav)),
        (b"RIFF\0\0\0\0AVI ", Some(Format::Avi)),
        (b"ID3\x04", Some(Format::Mp3)),
        (&[0xff, 0xfb, 0x90], Some(Format::Mp3)),
        (b"\0\0\0\x20ftypM4A ", Some(Format::M4a)),
        (b"\0\0\0\x20ftypisom", Some(Format::Mp4)),
        (&[0, 0, 1, 0xba], Some(Format::Mpeg)),
        (b"%PDF-1.7", None),
        (b"", None),
    ];
    for (head, want) in cases {
        assert_eq!(sniff(head), *want, "{head:?}");
    }
}

#[test]
fn a_misnamed_image_is_still_an_image() {
    let dir = Dir::new("misnamed");
    let a = Attachment::file(dir.write("shot.txt", PNG)).unwrap();
    assert_eq!(a.kind(), AttachmentKind::Image);
    assert_eq!(a.format.extension(), "png");
    assert!(a.path().is_absolute());
}

#[test]
fn files_a_notification_cannot_show_are_refused() {
    let dir = Dir::new("refused");
    let pdf = dir.write("doc.pdf", b"%PDF-1.7\n");
    assert_eq!(Attachment::file(&pdf), Err(AttachmentError::Unsupported { path: pdf }));
    assert!(matches!(Attachment::file(dir.0.join("gone.png")), Err(AttachmentError::Unreadable { .. })));
    assert!(matches!(Attachment::file(&dir.0), Err(AttachmentError::Unreadable { .. })));
}

#[test]
fn a_file_over_its_kinds_limit_is_refused() {
    let dir = Dir::new("large");
    let limit = attachment_limit(AttachmentKind::Audio);
    let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
    wav.resize(limit as usize + 1, 0);
    let path = dir.write("long.wav", &wav);
    assert_eq!(Attachment::file(&path), Err(AttachmentError::TooLarge { path, size: limit + 1, limit }));
    wav.truncate(limit as usize);
    assert!(Attachment::file(dir.write("fits.wav", &wav)).is_ok());
}

#[test]
fn staging_copies_under_the_formats_extension_and_leaves_the_original() {
    let dir = Dir::new("stage");
    let original = dir.write("shot", PNG);
    let a = Attachment::file(&original).unwrap();
    let staged = dir.0.join("staged");
    let first = stage(&a, &staged).unwrap();
    let second = stage(&a, &staged).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.extension().unwrap(), "png");
    assert_eq!(std::fs::read(&first).unwrap(), PNG);
    assert_eq!(std::fs::read(&original).unwrap(), PNG);
}

#[test]
fn a_file_changed_since_it_was_checked_is_not_staged() {
    let dir = Dir::new("changed");
    let path = dir.write("shot.png", PNG);
    let a = Attachment::file(&path).unwrap();
    std::fs::write(&path, b"%PDF-1.7\n").unwrap();
    let staged = dir.0.join("staged");
    assert_eq!(stage(&a, &staged), Err(AttachmentError::Unsupported { path: path.clone() }));
    assert_eq!(std::fs::read_dir(&staged).unwrap().count(), 0, "the refused copy is removed");
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(stage(&a, &staged), Err(AttachmentError::Unreadable { .. })));
}

#[cfg(unix)]
#[test]
fn unix_file_uris_escape_every_reserved_byte() {
    assert_eq!(file_uri(Path::new("/tmp/a b/shot#1.png")), "file:///tmp/a%20b/shot%231.png");
    assert_eq!(file_uri(Path::new("/tmp/a\\b.png")), "file:///tmp/a%5Cb.png");
}

#[cfg(windows)]
#[test]
fn windows_file_uris_keep_the_drive() {
    assert_eq!(file_uri(Path::new(r"C:\Data\a b\shot.png")), "file:///C:/Data/a%20b/shot.png");
}

#[cfg(unix)]
#[test]
fn file_uris_keep_bytes_that_are_not_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/shot\xff.png".to_vec()));
    assert_eq!(file_uri(&path), "file:///tmp/shot%FF.png");
}
