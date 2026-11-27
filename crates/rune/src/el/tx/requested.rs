use std::time::Duration;

use crate::el::FocusAsk;
use crate::{LiveRegion, Shell, Tx};

impl Tx {
    /// A `Tx` as `poll` gets one at `clock` seconds, for calling an app's `poll` or handlers
    /// directly in a test: [`Tx::clock`] reads `clock`, and [`Tx::wake_at`] measures from it.
    ///
    /// ```
    /// use std::time::Duration;
    /// use rune::Tx;
    ///
    /// let mut tx = Tx::at(10.0);
    /// tx.wake_at(10.25);
    /// assert_eq!(tx.requested().next_wake(), Some(Duration::from_millis(250)));
    /// ```
    pub fn at(clock: f64) -> Tx {
        Tx { clock, ..Tx::default() }
    }

    /// What this handler or `poll` has asked of the runtime so far.
    pub fn requested(&self) -> Requested<'_> {
        Requested(self)
    }
}

/// What a handler or `poll` asked of the runtime through its [`Tx`], read in a test
/// ([`Tx::requested`]). Each reads as the runtime would apply it: the last of
/// [`Tx::instant`], [`Tx::animated`] and [`Tx::settle`] wins, and so does the focus request
/// the runtime carries out.
///
/// ```
/// use rune::{FocusKind, LiveRegion, Tx};
///
/// let mut tx = Tx::default();
/// tx.animated();
/// tx.instant();
/// tx.focus("search", true);
/// tx.focus_when_shown("menu", false);
/// tx.announce("Moved", LiveRegion::Polite);
/// let asked = tx.requested();
/// assert!(asked.instant() && !asked.animated());
/// let focus = asked.focus().unwrap();
/// assert_eq!((focus.key, focus.ring, focus.kind), ("menu", false, FocusKind::WhenShown));
/// assert_eq!(asked.announcement(), Some(("Moved", LiveRegion::Polite)));
///
/// let mut tx = Tx::default();
/// tx.settle();
/// tx.animated();
/// assert!(tx.requested().settle() && tx.requested().animated());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Requested<'a>(&'a Tx);

/// A request to scroll a keyed row into view ([`Tx::scroll_into_view`],
/// [`Tx::scroll_into_view_in`], [`Tx::scroll_center`] and kin).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevealRequest<'a> {
    /// The element keyed `scope` the reveal is confined to, if any.
    pub scope: Option<&'a str>,
    /// The row to reveal.
    pub key: &'a str,
    /// Centre the row rather than scroll it just into view.
    pub center: bool,
    /// Glide there rather than jump.
    pub animated: bool,
}

/// Which call moved focus ([`Requested::focus`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKind {
    /// [`Tx::focus`].
    Focus,
    /// [`Tx::focus_when_shown`].
    WhenShown,
    /// [`Tx::return_focus`].
    Return,
}

/// A request to move focus to a keyed element ([`Requested::focus`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusRequest<'a> {
    /// The element focus moves to.
    pub key: &'a str,
    /// Whether its focus ring shows.
    pub ring: bool,
    /// Which call asked.
    pub kind: FocusKind,
}

impl<'a> Requested<'a> {
    /// Every wake asked for ([`Tx::wake_after`], and [`Tx::wake_at`] as the time left), in order.
    pub fn wakes(&self) -> impl Iterator<Item = Duration> + 'a {
        let clock = self.0.clock;
        self.0.wakes.iter().map(move |wake| wake.left(clock))
    }

    /// The soonest wake asked for.
    pub fn next_wake(&self) -> Option<Duration> {
        self.wakes().min()
    }

    /// [`Tx::request_poll`] was called.
    pub fn poll(&self) -> bool {
        self.0.request_poll
    }

    /// This change applies at once ([`Tx::instant`] or [`Tx::settle`]).
    pub fn instant(&self) -> bool {
        self.0.commit.snaps()
    }

    /// This change lands every running track ([`Tx::settle`]).
    pub fn settle(&self) -> bool {
        self.0.commit.lands()
    }

    /// This change animates because [`Tx::animated`] asked, and no later [`Tx::instant`] or
    /// [`Tx::settle`] made it apply at once.
    pub fn animated(&self) -> bool {
        self.0.asked_motion && !self.0.commit.snaps()
    }

    /// [`Tx::live_only`] was called: the change touched nothing the view reads.
    pub fn live_only(&self) -> bool {
        self.0.live_only
    }

    /// Where focus moves: the last [`Tx::focus`] or [`Tx::focus_when_shown`], else the last
    /// [`Tx::return_focus`].
    pub fn focus(&self) -> Option<FocusRequest<'a>> {
        self.0.focus_to.as_ref().map(|(key, ring, ask)| FocusRequest {
            key,
            ring: *ring,
            kind: match ask {
                FocusAsk::Focus => FocusKind::Focus,
                FocusAsk::WhenShown => FocusKind::WhenShown,
                FocusAsk::Return => FocusKind::Return,
            },
        })
    }

    /// [`Tx::blur`] was called.
    pub fn blur(&self) -> bool {
        self.0.blur
    }

    /// Rows to scroll into view, in order.
    pub fn reveals(&self) -> impl Iterator<Item = RevealRequest<'a>> + 'a {
        self.0.scroll_to.iter().map(|r| RevealRequest { scope: r.scope.as_deref(), key: &r.key, center: r.center, animated: r.animated })
    }

    /// Viewports sent to their bottom ([`Tx::scroll_to_bottom`] and kin), with whether each
    /// glides there.
    pub fn scrolls_to_bottom(&self) -> impl Iterator<Item = (&'a str, bool)> + 'a {
        self.0.scroll_bottom.iter().map(|(key, animated)| (key.as_str(), *animated))
    }

    /// Whether the window closes or the app quits after this handler ([`Tx::close_window`],
    /// [`Tx::quit`], [`Tx::relaunch`]).
    pub fn shell(&self) -> Option<Shell> {
        self.0.shell
    }

    /// The window title set ([`Tx::set_title`]).
    pub fn title(&self) -> Option<&'a str> {
        self.0.title.as_deref()
    }

    /// What a screen reader is asked to say, and how urgently ([`Tx::announce`]).
    pub fn announcement(&self) -> Option<(&'a str, LiveRegion)> {
        self.0.announce.as_ref().map(|(text, how)| (text.as_str(), *how))
    }
}
