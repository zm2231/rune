use super::*;

impl RuntimeCore {
    #[cfg(any(not(target_arch = "wasm32"), test))]
    pub(crate) fn cancel_shares(&mut self, app: &mut dyn AppDriver) {
        for _ in 0..64 {
            self.dismiss_share();
            let Some(pos) = self.frame.queue.iter().position(|a| matches!(a, Action::Shared(..))) else { return };
            let Action::Shared(f, result) = self.frame.queue.remove(pos) else { unreachable!() };
            let mut one = self.transaction(app);
            f(app.states(), result, &mut one);
            self.latch_requests(app, &mut one);
            merge(&mut self.frame.pending, one);
            self.commit.dirty = true;
        }
        eprintln!("rune: share callbacks kept sharing while their window went away; dropping them");
        self.drop_shares();
        self.frame.queue.retain(|a| !matches!(a, Action::Shared(..)));
    }
}
