//! The links a scene is handed, for `App::on_open_url`: the URLs opened with the app (a custom
//! scheme, `myapp://...`) and the web pages of the activities it is asked to continue (a
//! universal link, `https://...`).

/// The type of the activity a universal link arrives as: the value of Foundation's
/// `NSUserActivityTypeBrowsingWeb`.
pub(crate) const BROWSING_WEB: &str = "NSUserActivityTypeBrowsingWeb";

/// What a scene's delegate reads of an `NSUserActivity`.
pub(crate) struct Activity {
    /// Its `activityType`.
    pub(crate) kind: String,
    /// Its `webpageURL`, as a string.
    pub(crate) page: Option<String>,
}

/// The links in what a scene was handed at once: the opened URLs, then the page of each
/// activity that is a universal link. UIKit hands both over as sets, so the order within each
/// kind is whatever the set gave; links handed over at different times keep the order they
/// came in. Another kind of activity (Handoff, a Spotlight result) carries no link, and an
/// empty string is none.
pub(crate) fn incoming(urls: Vec<String>, activities: Vec<Activity>) -> Vec<String> {
    let pages = activities.into_iter().filter(|activity| activity.kind == BROWSING_WEB).filter_map(|activity| activity.page);
    urls.into_iter().chain(pages).filter(|link| !link.is_empty()).collect()
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
