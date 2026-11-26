#![allow(unsafe_code)]

use super::{Activity, BROWSING_WEB, incoming};

fn activity(kind: &str, page: Option<&str>) -> Activity {
    Activity { kind: kind.to_owned(), page: page.map(str::to_owned) }
}

#[test]
fn opened_urls_come_before_universal_links_each_kind_as_its_set_gave_it() {
    let urls = vec!["myapp://one".to_owned(), "myapp://two?x=1".to_owned()];
    let activities = vec![activity(BROWSING_WEB, Some("https://example.com/item/7"))];
    assert_eq!(incoming(urls, activities), ["myapp://one", "myapp://two?x=1", "https://example.com/item/7"]);
}

#[test]
fn an_activity_that_is_not_a_universal_link_carries_no_link() {
    let activities = vec![
        activity("com.example.myapp.editing", Some("https://example.com/handoff")),
        activity(BROWSING_WEB, Some("https://example.com/a")),
        activity("com.apple.corespotlightitem", None),
    ];
    assert_eq!(incoming(Vec::new(), activities), ["https://example.com/a"]);
}

#[test]
fn a_universal_link_with_no_page_and_an_empty_url_are_no_links() {
    let activities = vec![activity(BROWSING_WEB, None), activity(BROWSING_WEB, Some(""))];
    assert!(incoming(vec![String::new()], activities).is_empty());
}

#[test]
fn a_link_is_passed_through_as_given() {
    let link = "myapp://open?next=https%3A%2F%2Fexample.com%2F&q=a b#frag";
    assert_eq!(incoming(vec![link.to_owned()], Vec::new()), [link]);
}

/// Foundation is the same on a Mac, where the tests run.
#[cfg(target_os = "macos")]
#[test]
fn the_browsing_web_type_is_foundations_constant() {
    use objc2_foundation::NSString;

    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {
        static NSUserActivityTypeBrowsingWeb: &'static NSString;
    }
    // SAFETY: Foundation exports the constant as an `NSString *` that lives for the process.
    assert_eq!(unsafe { NSUserActivityTypeBrowsingWeb }.to_string(), BROWSING_WEB);
}
