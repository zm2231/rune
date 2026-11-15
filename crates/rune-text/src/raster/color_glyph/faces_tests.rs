use std::cell::Cell;

use super::faces::*;

/// Opens faces named by their key's index, counting how many it opened.
struct Opener(Cell<usize>);

impl Opener {
    fn get(&self, faces: &mut Faces<u32>, key: (u64, u32)) -> Option<u32> {
        faces
            .get(key, || {
                self.0.set(self.0.get() + 1);
                Some(key.1)
            })
            .copied()
    }
}

#[test]
fn a_face_is_opened_once() {
    let (mut faces, opener) = (Faces::default(), Opener(Cell::new(0)));
    assert_eq!(opener.get(&mut faces, (7, 0)), Some(0));
    assert_eq!(opener.get(&mut faces, (7, 0)), Some(0));
    assert_eq!((opener.0.get(), faces.len()), (1, 1));
}

#[test]
fn a_blob_and_an_index_are_one_face() {
    let (mut faces, opener) = (Faces::default(), Opener(Cell::new(0)));
    assert_eq!(opener.get(&mut faces, (7, 0)), Some(0));
    assert_eq!(opener.get(&mut faces, (7, 1)), Some(1));
    assert_eq!(opener.get(&mut faces, (8, 0)), Some(0));
    assert_eq!((opener.0.get(), faces.len()), (3, 3));
}

#[test]
fn no_more_than_the_bound_is_kept() {
    let (mut faces, opener) = (Faces::default(), Opener(Cell::new(0)));
    for blob in 0..3 * MAX_FACES as u64 {
        opener.get(&mut faces, (blob, 0));
        assert!(faces.len() <= MAX_FACES);
    }
    assert_eq!(faces.len(), MAX_FACES);
}

#[test]
fn the_face_used_longest_ago_leaves_first() {
    let (mut faces, opener) = (Faces::default(), Opener(Cell::new(0)));
    for blob in 0..MAX_FACES as u64 {
        opener.get(&mut faces, (blob, 0));
    }
    // Blob 0 is used again, so blob 1 is now the one used longest ago.
    opener.get(&mut faces, (0, 0));
    opener.get(&mut faces, (100, 0));
    let opened = opener.0.get();
    opener.get(&mut faces, (0, 0));
    assert_eq!(opener.0.get(), opened, "the face used again stayed");
    opener.get(&mut faces, (1, 0));
    assert_eq!(opener.0.get(), opened + 1, "the face used longest ago left and is opened again");
    assert_eq!(faces.len(), MAX_FACES);
}

#[test]
fn a_face_that_fails_to_open_is_asked_once() {
    let mut faces: Faces<u32> = Faces::default();
    let asked = Cell::new(0);
    for _ in 0..3 {
        let face = faces.get((7, 0), || {
            asked.set(asked.get() + 1);
            None
        });
        assert!(face.is_none());
    }
    assert_eq!((asked.get(), faces.len()), (1, 1));
}
