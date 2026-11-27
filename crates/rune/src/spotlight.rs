#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use crate::app::{App, AppHandle};
use crate::el::Tx;
use crate::open_url::incoming::{Glue, Inbox, Listening};

#[cfg(target_vendor = "apple")]
pub(crate) mod apple;

/// The most items one call to the system's index carries; a larger request is split.
pub const SEARCH_INDEX_BATCH: usize = 256;
/// The most index requests waiting for the indexer at once, counting each item and removal.
/// Past it, new requests are refused with [`SearchIndexError::Busy`].
pub const SEARCH_INDEX_PENDING: usize = 4096;
/// The largest thumbnail file an item may name; a larger one is left out of the item.
pub const SEARCH_THUMBNAIL_BYTES: u64 = 1 << 20;

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub(crate) const SPOTLIGHT_ACTIVITY: &str = "com.apple.corespotlightitem";

/// An item for the system's search (Spotlight on macOS and iOS): found by its title, details
/// and keywords, and opened in the app with [`App::on_open_search_item`], which gets its `id`.
///
/// ```
/// # use rune::SearchItem;
/// let item = SearchItem::new("note-42").with_domain("notes").with_title("Groceries").with_keyword("shopping");
/// assert_eq!(item.id(), "note-42");
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchItem {
    id: String,
    domain: Option<String>,
    title: Option<String>,
    details: Option<String>,
    keywords: Vec<String>,
    thumbnail: Option<PathBuf>,
}

impl SearchItem {
    pub fn new(id: impl Into<String>) -> Self { Self { id: id.into(), ..Self::default() } }

    /// A group of items the app can remove at once with [`Tx::remove_search_domain`].
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// A word the item is found by, besides its title and details. Give each word its own
    /// keyword: a phrase is one keyword, which a search for one of its words may not match.
    pub fn with_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.keywords.push(keyword.into());
        self
    }

    /// An image file shown beside the result, up to [`SEARCH_THUMBNAIL_BYTES`].
    pub fn with_thumbnail(mut self, path: impl Into<PathBuf>) -> Self {
        self.thumbnail = Some(path.into());
        self
    }

    pub fn id(&self) -> &str { &self.id }

    pub fn domain(&self) -> Option<&str> { self.domain.as_deref() }

    pub fn title(&self) -> Option<&str> { self.title.as_deref() }

    pub fn details(&self) -> Option<&str> { self.details.as_deref() }

    pub fn keywords(&self) -> &[String] { &self.keywords }

    pub fn thumbnail(&self) -> Option<&std::path::Path> { self.thumbnail.as_deref() }
}

/// Why the system's search index didn't take a request, for [`App::on_search_index_error`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchIndexError {
    /// [`SEARCH_INDEX_PENDING`] requests already wait; this many items or removals were dropped.
    Busy(usize),
    /// The item's thumbnail is missing or larger than [`SEARCH_THUMBNAIL_BYTES`]; the item was
    /// indexed without it.
    Thumbnail(String),
    /// The system refused the request.
    Failed(String),
}

impl std::fmt::Display for SearchIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy(dropped) => write!(f, "the search index is busy; {dropped} requests were dropped"),
            Self::Thumbnail(id) => write!(f, "the thumbnail of search item {id} is missing or too large"),
            Self::Failed(why) => write!(f, "the search index refused a request: {why}"),
        }
    }
}
impl std::error::Error for SearchIndexError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IndexOp {
    Index(Vec<SearchItem>),
    Remove(Vec<String>),
    RemoveDomain(String),
}

impl IndexOp {
    pub(crate) fn weight(&self) -> usize {
        match self {
            Self::Index(items) => items.len(),
            Self::Remove(ids) => ids.len(),
            Self::RemoveDomain(_) => 1,
        }
    }
}

#[derive(Default)]
pub(crate) struct Queue {
    ops: VecDeque<IndexOp>,
    weight: usize,
}

impl Queue {
    #[cfg_attr(not(any(test, target_vendor = "apple")), allow(dead_code))]
    pub(crate) fn admit(&mut self, ops: Vec<IndexOp>) -> usize {
        let mut dropped = 0;
        for op in ops {
            let weight = op.weight();
            if self.weight + weight > SEARCH_INDEX_PENDING {
                dropped += weight;
                continue;
            }
            self.weight += weight;
            self.ops.push_back(op);
        }
        dropped
    }

    #[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
    pub(crate) fn next(&mut self) -> Option<IndexOp> {
        let op = self.ops.pop_front()?;
        self.weight -= op.weight();
        Some(op)
    }
}

pub(crate) type OnOpen<S> = fn(&mut S, &str, &mut Tx);
pub(crate) type OnError<S> = fn(&mut S, &SearchIndexError, &mut Tx);

static OPENED: Inbox<String> = Inbox::new();
static ERRORS: Inbox<SearchIndexError> = Inbox::new();

struct Always;

impl Glue for Always {
    fn handled_elsewhere(&self) -> bool { false }
    fn install(&self) -> bool { cfg!(target_vendor = "apple") }
    fn remove(&self) {}
}

pub(crate) fn prepare() {
    OPENED.prepare(&Always);
    ERRORS.prepare(&Always);
}

pub(crate) fn settle() {
    OPENED.settle(&Always);
    ERRORS.settle(&Always);
}

pub(crate) struct Listeners {
    _opened: Option<Listening<String>>,
    _errors: Option<Listening<SearchIndexError>>,
}

pub(crate) fn listen<S: 'static>(app: &App<S>) -> Listeners {
    Listeners { _opened: listen_opened(&OPENED, app, &Always), _errors: listen_errors(&ERRORS, app, &Always) }
}

pub(crate) fn listen_opened<S: 'static>(inbox: &'static Inbox<String>, app: &App<S>, glue: &dyn Glue) -> Option<Listening<String>> {
    let hook = app.on_open_search_item?;
    let handle: AppHandle<S> = app.handle();
    inbox.listen_sink(glue, Arc::new(move |id: String| {
        handle.update(move |s, tx| {
            #[cfg(feature = "hot")]
            crate::hot::patched_or_original!("on_open_search_item", hook, s, id.as_str(), tx);
            #[cfg(not(feature = "hot"))]
            hook(s, &id, tx);
        });
    }))
}

pub(crate) fn listen_errors<S: 'static>(inbox: &'static Inbox<SearchIndexError>, app: &App<S>, glue: &dyn Glue) -> Option<Listening<SearchIndexError>> {
    let hook = app.on_search_index_error?;
    let handle: AppHandle<S> = app.handle();
    inbox.listen_sink(glue, Arc::new(move |error: SearchIndexError| {
        handle.update(move |s, tx| {
            #[cfg(feature = "hot")]
            crate::hot::patched_or_original!("on_search_index_error", hook, s, &error, tx);
            #[cfg(not(feature = "hot"))]
            hook(s, &error, tx);
        });
    }))
}

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub(crate) fn opened(id: String) { OPENED.receive(id); }

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub(crate) fn failed(error: SearchIndexError) {
    eprintln!("rune: {error}");
    ERRORS.receive(error);
}

pub(crate) fn submit(ops: Vec<IndexOp>) {
    #[cfg(target_vendor = "apple")]
    apple::submit(ops);
    #[cfg(not(target_vendor = "apple"))]
    let _ = ops;
}

impl crate::runtime::RuntimeCore {
    pub(crate) fn request_search_index(&mut self, ops: Vec<IndexOp>) {
        if ops.is_empty() { return; }
        if self.host.open_urls_natively { submit(ops); } else { self.host.search_index_requests.extend(ops); }
    }
}

impl Tx {
    /// Adds `items` to the system's search index once this handler's frame is done, replacing
    /// items with the same ids. The work runs off the main thread in batches of
    /// [`SEARCH_INDEX_BATCH`]; failures reach [`App::on_search_index_error`]. macOS and iOS
    /// (Core Spotlight); other platforms ignore it.
    ///
    /// ```
    /// # use rune::{SearchItem, Tx, el};
    /// # let _ = el()
    /// .on_press_tx(|_: &mut (), tx: &mut Tx| {
    ///     tx.index_search_items([SearchItem::new("note-42").with_title("Groceries")]);
    /// })
    /// # ;
    /// ```
    pub fn index_search_items(&mut self, items: impl IntoIterator<Item = SearchItem>) {
        let items: Vec<_> = items.into_iter().collect();
        if !items.is_empty() { self.search_index.push(IndexOp::Index(items)); }
    }

    /// Removes the items with these ids from the system's search index.
    pub fn remove_search_items(&mut self, ids: impl IntoIterator<Item = impl Into<String>>) {
        let ids: Vec<String> = ids.into_iter().map(Into::into).collect();
        if !ids.is_empty() { self.search_index.push(IndexOp::Remove(ids)); }
    }

    /// Removes every item indexed with [`SearchItem::with_domain`] `domain`.
    pub fn remove_search_domain(&mut self, domain: impl Into<String>) { self.search_index.push(IndexOp::RemoveDomain(domain.into())); }
}

#[cfg(test)]
mod tests;
