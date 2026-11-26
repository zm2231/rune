use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    Audio,
    Video,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Png,
    Jpeg,
    Gif,
    Aiff,
    Wav,
    Mp3,
    M4a,
    Mp4,
    Mpeg,
    Avi,
}

impl Format {
    pub(crate) fn kind(self) -> AttachmentKind {
        match self {
            Format::Png | Format::Jpeg | Format::Gif => AttachmentKind::Image,
            Format::Aiff | Format::Wav | Format::Mp3 | Format::M4a => AttachmentKind::Audio,
            Format::Mp4 | Format::Mpeg | Format::Avi => AttachmentKind::Video,
        }
    }

    #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
    pub(crate) fn uniform_type(self) -> &'static str {
        match self {
            Format::Png => "public.png",
            Format::Jpeg => "public.jpeg",
            Format::Gif => "com.compuserve.gif",
            Format::Aiff => "public.aiff-audio",
            Format::Wav => "com.microsoft.waveform-audio",
            Format::Mp3 => "public.mp3",
            Format::M4a => "com.apple.m4a-audio",
            Format::Mp4 => "public.mpeg-4",
            Format::Mpeg => "public.mpeg",
            Format::Avi => "public.avi",
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Gif => "gif",
            Format::Aiff => "aiff",
            Format::Wav => "wav",
            Format::Mp3 => "mp3",
            Format::M4a => "m4a",
            Format::Mp4 => "mp4",
            Format::Mpeg => "mpg",
            Format::Avi => "avi",
        }
    }
}

const HEAD: usize = 12;

pub(crate) fn sniff(head: &[u8]) -> Option<Format> {
    let at = |i: usize, tag: &[u8]| head.get(i..i + tag.len()) == Some(tag);
    if at(0, b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if at(0, &[0xff, 0xd8, 0xff]) {
        Some(Format::Jpeg)
    } else if at(0, b"GIF87a") || at(0, b"GIF89a") {
        Some(Format::Gif)
    } else if at(0, b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
        Some(Format::Aiff)
    } else if at(0, b"RIFF") && at(8, b"WAVE") {
        Some(Format::Wav)
    } else if at(0, b"RIFF") && at(8, b"AVI ") {
        Some(Format::Avi)
    } else if at(4, b"ftyp") {
        Some(if at(8, b"M4A ") || at(8, b"M4B ") { Format::M4a } else { Format::Mp4 })
    } else if at(0, &[0, 0, 1, 0xba]) || at(0, &[0, 0, 1, 0xb3]) {
        Some(Format::Mpeg)
    } else if at(0, b"ID3") || matches!(head, [0xff, b, ..] if b & 0xe0 == 0xe0) {
        Some(Format::Mp3)
    } else {
        None
    }
}

/// The largest file of `kind` a notification takes: Apple's limits (10 MB images, 5 MB sound,
/// 50 MB video), and 3 MB images on Windows.
pub fn attachment_limit(kind: AttachmentKind) -> u64 {
    const MB: u64 = 1_000_000;
    match kind {
        AttachmentKind::Image if cfg!(target_os = "windows") => 3 * MB,
        AttachmentKind::Image => 10 * MB,
        AttachmentKind::Audio => 5 * MB,
        AttachmentKind::Video => 50 * MB,
    }
}

/// Why a file can't be attached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentError {
    Unreadable { path: PathBuf, reason: String },
    /// Not a PNG, JPEG or GIF image, an AIFF, WAV, MP3 or M4A sound, or an MP4, MPEG or AVI video.
    Unsupported { path: PathBuf },
    TooLarge { path: PathBuf, size: u64, limit: u64 },
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttachmentError::Unreadable { path, reason } => write!(f, "can't read {}: {reason}", path.display()),
            AttachmentError::Unsupported { path } => write!(f, "{} is not an image, sound or video a notification shows", path.display()),
            AttachmentError::TooLarge { path, size, limit } => write!(f, "{} is {size} bytes, over the {limit}-byte limit", path.display()),
        }
    }
}

impl std::error::Error for AttachmentError {}

/// A file a notification shows (`Notification::attachment`), checked by `Attachment::file`.
#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    path: PathBuf,
    format: Format,
}

impl Attachment {
    #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
    pub(crate) fn uniform_type(&self) -> &'static str {
        self.format.uniform_type()
    }
}

impl Attachment {
    /// The file at `path`, once its first bytes show a format a notification takes and its size
    /// is within `attachment_limit`. It opens the file: call it off the UI thread.
    pub fn file(path: impl AsRef<Path>) -> Result<Attachment, AttachmentError> {
        let path = std::path::absolute(path.as_ref()).unwrap_or_else(|_| path.as_ref().to_owned());
        let format = check(&path)?;
        Ok(Attachment { path, format })
    }

    pub fn kind(&self) -> AttachmentKind {
        self.format.kind()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn check(path: &Path) -> Result<Format, AttachmentError> {
    let unreadable = |e: std::io::Error| AttachmentError::Unreadable { path: path.to_owned(), reason: e.to_string() };
    let mut file = std::fs::File::open(path).map_err(unreadable)?;
    let meta = file.metadata().map_err(unreadable)?;
    if !meta.is_file() {
        return Err(AttachmentError::Unreadable { path: path.to_owned(), reason: "not a file".into() });
    }
    let mut head = Vec::with_capacity(HEAD);
    file.by_ref().take(HEAD as u64).read_to_end(&mut head).map_err(unreadable)?;
    let format = sniff(&head).ok_or_else(|| AttachmentError::Unsupported { path: path.to_owned() })?;
    let limit = attachment_limit(format.kind());
    if meta.len() > limit {
        return Err(AttachmentError::TooLarge { path: path.to_owned(), size: meta.len(), limit });
    }
    Ok(format)
}

#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn stage(a: &Attachment, dir: &Path) -> Result<PathBuf, AttachmentError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unreadable = |e: std::io::Error| AttachmentError::Unreadable { path: a.path.clone(), reason: e.to_string() };
    std::fs::create_dir_all(dir).map_err(unreadable)?;
    let name = format!("{}-{}.{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed), a.format.extension());
    let copy = dir.join(name);
    std::fs::copy(&a.path, &copy).map_err(unreadable)?;
    match check(&copy) {
        Ok(format) if format == a.format => Ok(copy),
        Ok(_) => {
            std::fs::remove_file(&copy).ok();
            Err(AttachmentError::Unsupported { path: a.path.clone() })
        }
        Err(e) => {
            std::fs::remove_file(&copy).ok();
            Err(match e {
                AttachmentError::TooLarge { size, limit, .. } => AttachmentError::TooLarge { path: a.path.clone(), size, limit },
                AttachmentError::Unsupported { .. } => AttachmentError::Unsupported { path: a.path.clone() },
                e => e,
            })
        }
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
pub(crate) fn file_uri(path: &Path) -> String {
    #[cfg(unix)]
    let (drive, rest): (Vec<u8>, Vec<u8>) = (Vec::new(), std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()).to_vec());
    #[cfg(not(unix))]
    let (drive, rest) = windows_parts(path);
    let mut out = String::from("file://");
    if !drive.is_empty() {
        out.push('/');
        out.push_str(&String::from_utf8_lossy(&drive));
    }
    for b in rest {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(not(unix))]
fn windows_parts(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    match s.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => (s.as_bytes()[..2].to_vec(), s.as_bytes()[2..].to_vec()),
        _ => (Vec::new(), s.as_bytes().to_vec()),
    }
}

#[cfg(test)]
#[path = "attachment_tests.rs"]
mod tests;
