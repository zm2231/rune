use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Done;
use crate::open_url::{Link, OpenUrlError, opened};
use crate::{Env, Runtime, Tx, el};

#[derive(Default)]
struct App {
    heard: Vec<Result<(), OpenUrlError>>,
}

fn link(url: &str) -> Link {
    Link { url: url.into(), same_tab: false, opened: Some(opened(|s: &mut App, result, _: &mut Tx| s.heard.push(result))) }
}

fn heard(rt: &mut Runtime<App>) -> Vec<Result<(), OpenUrlError>> {
    let mut heard = Vec::new();
    rt.update(|s, _| heard = s.heard.clone());
    heard
}

#[test]
fn a_link_hears_the_systems_answer_on_the_frame_after_it_comes() {
    let mut rt = Runtime::new(App::default(), |_: &App, _| el(), (100.0, 100.0), Env::new(false, 1.0));
    rt.frame(0.0);
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = woken.clone();
    rt.core_mut().answer_links_later(Arc::new(move || { counter.fetch_add(1, Ordering::Relaxed); }));
    let asked: Rc<RefCell<Vec<(String, Done)>>> = Rc::default();
    let opener = asked.clone();
    rt.core_mut().open_urls_later_with(vec![link("https://example.com/"), link("mailto:a@example.com")], move |url, done| opener.borrow_mut().push((url.into(), done)));

    rt.frame(1.0 / 60.0);
    assert!(heard(&mut rt).is_empty(), "nothing is heard before the system answers");

    let mut asked = std::mem::take(&mut *asked.borrow_mut());
    let (mail, mail_done) = asked.pop().unwrap();
    let (web, web_done) = asked.pop().unwrap();
    assert_eq!((web.as_str(), mail.as_str()), ("https://example.com/", "mailto:a@example.com"));
    web_done(Ok(()));
    mail_done(Err(OpenUrlError::Failed("no app opened it".into())));
    assert_eq!(woken.load(Ordering::Relaxed), 2);

    rt.core_mut().take_late_link_answers();
    rt.frame(2.0 / 60.0);
    assert_eq!(heard(&mut rt), vec![Ok(()), Err(OpenUrlError::Failed("no app opened it".into()))]);
}
