//! Size classes: how much room a view has across and down, in the two steps iOS lays apps out
//! by ([`Cx::horizontal_size_class`](crate::Cx::horizontal_size_class)).

/// A size class: one of the two steps of room iOS names for each direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeClass {
    /// Little room: an iPhone's width, a narrow iPad window's width, a phone on its side's height.
    Compact,
    /// Room for more: a full-screen iPad, a wide window.
    Regular,
}

/// A view's size classes across and down, `None` where the platform names none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SizeClasses {
    pub horizontal: Option<SizeClass>,
    pub vertical: Option<SizeClass>,
}
