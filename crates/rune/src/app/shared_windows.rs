use std::sync::Arc;

use super::{App, RuntimeConfig, Shared};
use crate::shared_state::Member;
use crate::tooling::AppSpec;

type Parts<S> = (AppSpec<S>, S, Arc<Shared<S>>, RuntimeConfig<S>, Option<Member<S>>);

impl<S: 'static> App<S> {
    pub(crate) fn into_member_parts(mut self, key: &str, scene: Option<&str>) -> Parts<S> {
        let Some(shared) = self.shared_state.take() else {
            let (spec, state, app_shared, runtime) = self.into_parts(scene);
            return (spec, state, app_shared, runtime, None);
        };
        let member = shared.member(key);
        (self.spec, shared.placeholder(), self.shared, self.runtime, Some(member))
    }
}
