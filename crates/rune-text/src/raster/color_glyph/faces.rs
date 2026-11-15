//! The faces the platform's text system has opened to draw colour glyphs from, kept between
//! glyphs so a font's bytes are handed to it once, not once a glyph.

/// The faces kept. A fifth replaces the one used longest ago: a text draws its emoji from one
/// face, and an app's own colour fonts are few.
pub(super) const MAX_FACES: usize = 4;

/// The faces kept, by font blob id and face index: `F` is the platform's face.
///
/// A face holds no pixels. A glyph drawn from it is copied out into an image of its own, which
/// the colour glyph cache keeps and counts against its byte budget, so a face that leaves
/// here takes no drawn glyph with it; a glyph asked for again opens the face again. A blob's
/// id is never given to another blob, so an entry never answers for a font it wasn't made from.
///
/// A face does keep its whole font file in memory: the platform's face holds the data it was
/// opened from, which holds a clone of the font's blob. So each entry pins one blob until it
/// leaves here, and at most `MAX_FACES` blobs stay alive that their fonts' owners let go of.
pub(crate) struct Faces<F> {
    /// Least recently used first. None is a face the platform couldn't open, kept so that it
    /// is asked once and not for each of the font's glyphs.
    faces: Vec<((u64, u32), Option<F>)>,
}

impl<F> Default for Faces<F> {
    fn default() -> Self {
        Faces { faces: Vec::new() }
    }
}

impl<F> Faces<F> {
    /// The face of `key` (a font blob's id and the face's index in it): `open` makes it, the
    /// first time only, and None is a face it couldn't make.
    pub(super) fn get(&mut self, key: (u64, u32), open: impl FnOnce() -> Option<F>) -> Option<&F> {
        let entry = match self.faces.iter().position(|(k, _)| *k == key) {
            Some(at) => self.faces.remove(at),
            None => {
                if self.faces.len() == MAX_FACES {
                    self.faces.remove(0);
                }
                (key, open())
            }
        };
        self.faces.push(entry);
        self.faces.last().and_then(|(_, face)| face.as_ref())
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.faces.len()
    }
}
