#![allow(unsafe_code)]

use std::sync::mpsc;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::{Allocated, Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSString, NSURL};

use super::{IndexOp, Queue, SEARCH_INDEX_BATCH, SEARCH_THUMBNAIL_BYTES, SearchIndexError, SearchItem};

const ANSWER_WAIT: Duration = Duration::from_secs(30);

#[link(name = "CoreSpotlight", kind = "framework")]
unsafe extern "C" {
    static CSSearchableItemActivityIdentifier: &'static NSString;
}

pub(crate) fn activity_identifier_key() -> String {
    // SAFETY: a constant `NSString` Core Spotlight exports.
    unsafe { CSSearchableItemActivityIdentifier }.to_string()
}

struct Worker {
    queue: Mutex<Queue>,
    ready: Condvar,
}

fn worker() -> Result<&'static Worker, String> {
    static WORKER: OnceLock<Result<&'static Worker, String>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let worker: &'static Worker = Box::leak(Box::new(Worker { queue: Mutex::default(), ready: Condvar::new() }));
        let spawned = std::thread::Builder::new().name("rune-search-index".into()).spawn(move || run(worker));
        spawned.map(|_| worker).map_err(|e| format!("could not start the search indexer: {e}"))
    }).clone()
}

pub(super) fn submit(ops: Vec<IndexOp>) {
    let worker = match worker() {
        Ok(worker) => worker,
        Err(why) => return super::failed(SearchIndexError::Failed(why)),
    };
    let dropped = match worker.queue.lock() {
        Ok(mut queue) => queue.admit(ops),
        Err(_) => return,
    };
    worker.ready.notify_one();
    if dropped > 0 { super::failed(SearchIndexError::Busy(dropped)); }
}

fn run(worker: &'static Worker) {
    loop {
        let op = {
            let Ok(mut queue) = worker.queue.lock() else { return };
            loop {
                if let Some(op) = queue.next() { break op; }
                queue = match worker.ready.wait(queue) {
                    Ok(queue) => queue,
                    Err(_) => return,
                };
            }
        };
        autoreleasepool(|_| apply(op));
    }
}

fn strings(values: &[impl AsRef<str>]) -> Retained<AnyObject> {
    // SAFETY: an `NSMutableArray` of `NSString`s.
    unsafe {
        let array: Retained<AnyObject> = msg_send![class!(NSMutableArray), array];
        for value in values {
            let _: () = msg_send![&*array, addObject: &*NSString::from_str(value.as_ref())];
        }
        array
    }
}

fn thumbnail(item: &SearchItem) -> Option<Retained<NSURL>> {
    let path = item.thumbnail()?;
    let fits = std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() <= SEARCH_THUMBNAIL_BYTES);
    if !fits {
        super::failed(SearchIndexError::Thumbnail(item.id().to_owned()));
        return None;
    }
    Some(NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?)))
}

fn searchable(item: &SearchItem) -> Retained<AnyObject> {
    let kind = NSString::from_str("public.content");
    // SAFETY: `UTType typeWithIdentifier:` answers a type or nil; `initWithContentType:` makes a
    // `CSSearchableItemAttributeSet`, whose string setters take `NSString`s, `keywords` an
    // `NSArray` of them and `thumbnailURL` a file `NSURL`. `initWithUniqueIdentifier:
    // domainIdentifier:attributeSet:` makes a `CSSearchableItem`; the domain may be nil.
    unsafe {
        let content: Option<Retained<AnyObject>> = msg_send![class!(UTType), typeWithIdentifier: &*kind];
        let attributes: Allocated<AnyObject> = msg_send![class!(CSSearchableItemAttributeSet), alloc];
        let attributes: Retained<AnyObject> = msg_send![attributes, initWithContentType: content.as_deref()];
        if let Some(title) = item.title().map(NSString::from_str) {
            let _: () = msg_send![&*attributes, setTitle: &*title];
            let _: () = msg_send![&*attributes, setDisplayName: &*title];
        }
        if let Some(details) = item.details() { let _: () = msg_send![&*attributes, setContentDescription: &*NSString::from_str(details)]; }
        if !item.keywords().is_empty() { let _: () = msg_send![&*attributes, setKeywords: &*strings(item.keywords())]; }
        if let Some(url) = thumbnail(item) { let _: () = msg_send![&*attributes, setThumbnailURL: &*url]; }
        let id = NSString::from_str(item.id());
        let domain = item.domain().map(NSString::from_str);
        let made: Allocated<AnyObject> = msg_send![class!(CSSearchableItem), alloc];
        msg_send![made, initWithUniqueIdentifier: &*id, domainIdentifier: domain.as_deref(), attributeSet: &*attributes]
    }
}

fn answered(call: impl FnOnce(&block2::Block<dyn Fn(*mut AnyObject)>)) {
    let (send, answer) = mpsc::channel::<Option<String>>();
    let block = RcBlock::new(move |error: *mut AnyObject| {
        // SAFETY: Core Spotlight answers with an `NSError` or nil, whose `localizedDescription`
        // is an `NSString`.
        let why = unsafe { error.as_ref() }.map(|error| {
            // SAFETY: as above.
            let text: Retained<NSString> = unsafe { msg_send![error, localizedDescription] };
            text.to_string()
        });
        send.send(why).ok();
    });
    call(&block);
    match answer.recv_timeout(ANSWER_WAIT) {
        Ok(Some(why)) => super::failed(SearchIndexError::Failed(why)),
        Ok(None) => {}
        Err(_) => super::failed(SearchIndexError::Failed("the search index did not answer".into())),
    }
}

fn apply(op: IndexOp) {
    // SAFETY: `defaultSearchableIndex` is the app's `CSSearchableIndex`, safe to use from any
    // thread; its index and delete methods take arrays of items, identifiers or domains, and a
    // completion block that takes an `NSError` or nil.
    let index: Retained<AnyObject> = unsafe { msg_send![class!(CSSearchableIndex), defaultSearchableIndex] };
    match op {
        IndexOp::Index(items) => for chunk in items.chunks(SEARCH_INDEX_BATCH) {
            autoreleasepool(|_| {
                // SAFETY: as above, with an `NSMutableArray` of `CSSearchableItem`s.
                let array: Retained<AnyObject> = unsafe { msg_send![class!(NSMutableArray), array] };
                for item in chunk {
                    // SAFETY: as above.
                    unsafe { let _: () = msg_send![&*array, addObject: &*searchable(item)]; }
                }
                // SAFETY: as above.
                answered(|done| unsafe { let _: () = msg_send![&*index, indexSearchableItems: &*array, completionHandler: done]; });
            });
        },
        IndexOp::Remove(ids) => for chunk in ids.chunks(SEARCH_INDEX_BATCH) {
            let ids = strings(chunk);
            // SAFETY: as above.
            answered(|done| unsafe { let _: () = msg_send![&*index, deleteSearchableItemsWithIdentifiers: &*ids, completionHandler: done]; });
        },
        IndexOp::RemoveDomain(domain) => {
            let domains = strings(&[domain]);
            // SAFETY: as above.
            answered(|done| unsafe { let _: () = msg_send![&*index, deleteSearchableItemsWithDomainIdentifiers: &*domains, completionHandler: done]; });
        }
    }
}
