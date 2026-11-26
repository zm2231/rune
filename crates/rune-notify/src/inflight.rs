use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Post {
    Wanted(u64),
    Withdrawn(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settled {
    Wanted,
    Withdrawn,
    Superseded,
}

#[derive(Debug, Default)]
pub(crate) struct Inflight {
    next: u64,
    posts: HashMap<String, Post>,
}

impl Inflight {
    pub(crate) fn start(&mut self, id: &str) -> u64 {
        self.next += 1;
        self.posts.insert(id.to_owned(), Post::Wanted(self.next));
        self.next
    }

    pub(crate) fn withdraw(&mut self, id: &str) {
        if let Some(post) = self.posts.get_mut(id)
            && let Post::Wanted(ticket) = *post
        {
            *post = Post::Withdrawn(ticket);
        }
    }

    pub(crate) fn wanted(&self, id: &str, ticket: u64) -> bool {
        self.posts.get(id) == Some(&Post::Wanted(ticket))
    }

    pub(crate) fn settle(&mut self, id: &str, ticket: u64) -> Settled {
        let settled = match self.posts.get(id) {
            Some(Post::Wanted(t)) if *t == ticket => Settled::Wanted,
            Some(Post::Withdrawn(t)) if *t == ticket => Settled::Withdrawn,
            _ => return Settled::Superseded,
        };
        self.posts.remove(id);
        settled
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.posts.len()
    }
}

#[cfg(test)]
#[path = "inflight_tests.rs"]
mod tests;
