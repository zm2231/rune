use std::sync::{Mutex, OnceLock};

/// Whether the person lets the app's notifications show, from `authorization`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Authorization {
    /// The app can't post at all (`available` is false).
    Unavailable,
    /// Nothing is decided yet. macOS and iOS: the person hasn't been asked, and the next post
    /// asks. Windows: the system has no record of the app's toasts yet.
    NotDetermined,
    /// The person, or a policy, turned the app's notifications off: posts are dropped.
    Denied,
    /// Notifications show.
    Authorized,
    /// macOS and iOS: allowed provisionally, delivered quietly to Notification Center until the
    /// person keeps or turns them off.
    Provisional,
    /// iOS: an App Clip's notifications, allowed for a limited time.
    Ephemeral,
}

impl Authorization {
    /// Whether a post shows (quietly, for `Provisional`).
    pub fn allows_posts(self) -> bool {
        matches!(self, Authorization::Authorized | Authorization::Provisional | Authorization::Ephemeral)
    }
}

/// Why a notification that `post` took didn't show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The app's notifications are off (`Authorization::Denied`), or the person turned them down
    /// when the post asked.
    Denied,
    /// The system refused it, with its description of why.
    System(String),
}

/// A notification `post` took that the system didn't show, from `on_post_error`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostError {
    /// The id it was posted under (`Notification::new`).
    pub id: String,
    pub failure: Failure,
}

impl std::fmt::Display for PostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.failure {
            Failure::Denied => write!(f, "{} was not posted: notifications are off for this app", self.id),
            Failure::System(why) => write!(f, "{} was not posted: {why}", self.id),
        }
    }
}

impl std::error::Error for PostError {}

type ErrorHandler = Box<dyn Fn(PostError) + Send + Sync>;

pub(crate) fn error_handler() -> &'static Mutex<Option<ErrorHandler>> {
    static HANDLER: OnceLock<Mutex<Option<ErrorHandler>>> = OnceLock::new();
    HANDLER.get_or_init(|| Mutex::new(None))
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "ios", target_os = "windows")), allow(dead_code))]
pub(crate) fn report(id: &str, failure: Failure) {
    if let Some(f) = error_handler().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        f(PostError { id: id.to_owned(), failure });
    }
}
