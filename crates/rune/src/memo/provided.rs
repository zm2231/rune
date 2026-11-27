use super::{SCOPES, hash_of};

#[must_use]
pub(crate) struct Salt(Option<u64>);

impl Drop for Salt {
    fn drop(&mut self) {
        if let Some(env) = self.0 {
            SCOPES.with_borrow_mut(|scopes| {
                if let Some(s) = scopes.last_mut() {
                    s.env = env;
                }
            });
        }
    }
}

pub(crate) fn salt(id: u64) -> Salt {
    Salt(SCOPES.with_borrow_mut(|scopes| {
        scopes.last_mut().map(|s| {
            s.provided = s.provided.wrapping_add(hash_of(&id));
            let salted = hash_of(&(s.env, id));
            std::mem::replace(&mut s.env, salted)
        })
    }))
}

pub(crate) fn seal() {
    SCOPES.with_borrow_mut(|scopes| {
        if let Some(s) = scopes.last_mut() {
            s.cache.provided = s.provided;
            s.env = hash_of(&(s.env, s.provided));
        }
    });
}

pub(super) fn restore(provided: u64) {
    SCOPES.with_borrow_mut(|scopes| {
        if let Some(s) = scopes.last_mut() {
            s.provided = provided;
            s.cache.provided = provided;
            s.env = hash_of(&(s.env, provided));
        }
    });
}
