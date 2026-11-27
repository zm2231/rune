use crate::Tx;

impl Tx {
    /// The key of the window whose handler or `poll` this is: the one `WindowSpec::new` gave
    /// it, or `"main"` for `rune::run`'s window. `None` for a view embedded in another host and
    /// a `Tx` no runtime made. Windows over one [`SharedState`](crate::SharedState) tell their
    /// turns apart with it.
    pub fn window(&self) -> Option<&str> {
        self.window.as_deref()
    }
}
