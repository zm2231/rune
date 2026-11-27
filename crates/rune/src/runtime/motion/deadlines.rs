use std::time::Duration;

const SAME_INSTANT: f64 = 1e-6;
const STACKED_POLLS: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Wake {
    After(Duration),
    At(f64),
}

impl Wake {
    pub(crate) fn deadline(self, from: f64) -> f64 {
        match self {
            Wake::After(delay) => from + delay.as_secs_f64(),
            Wake::At(due) => due,
        }
    }

    pub(crate) fn due_by(self, from: f64, clock: f64) -> bool {
        self.deadline(from) <= clock
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn left(self, from: f64) -> Duration {
        Duration::from_secs_f64((self.deadline(from) - from).max(0.0))
    }
}

#[derive(Debug, Default)]
pub(crate) struct Deadlines {
    app: Vec<f64>,
    timers: Vec<f64>,
    poll_after: Vec<(Duration, f64)>,
    stacked_polls: u32,
}

impl Deadlines {
    pub(crate) fn insert_app(&mut self, deadline: f64) {
        insert(&mut self.app, deadline);
    }

    pub(crate) fn poll_asked(&mut self, clock: f64, wakes: &[Wake]) {
        let asked = wakes.iter().filter_map(|wake| match wake {
            Wake::After(delay) if !delay.is_zero() => Some(*delay),
            _ => None,
        });
        let stacks = asked.clone().any(|delay| {
            self.poll_after.iter().any(|&(earlier, due)| earlier == delay && due > clock && (clock + delay.as_secs_f64() - due).abs() > SAME_INSTANT)
        });
        self.stacked_polls = if stacks { self.stacked_polls + 1 } else { 0 };
        self.poll_after.clear();
        self.poll_after.extend(asked.map(|delay| (delay, clock + delay.as_secs_f64())));
        if self.polls_stacking() {
            crate::tooling::perf_warn::poll_wakes_pile_up();
        }
    }

    pub(crate) fn polls_stacking(&self) -> bool {
        self.stacked_polls >= STACKED_POLLS
    }

    pub(crate) fn insert_timer(&mut self, deadline: f64) {
        insert(&mut self.timers, deadline);
    }

    pub(crate) fn first(&self) -> Option<f64> {
        match (self.app.first(), self.timers.first()) {
            (Some(a), Some(t)) => Some(a.min(*t)),
            (a, t) => a.or(t).copied(),
        }
    }

    pub(crate) fn any_due(&self, clock: f64) -> bool {
        self.first().is_some_and(|t| t <= clock)
    }

    pub(crate) fn drain_due(&mut self, clock: f64) {
        for list in [&mut self.app, &mut self.timers] {
            let due = list.partition_point(|t| *t <= clock);
            list.drain(..due);
        }
    }

    #[cfg(test)]
    pub(crate) fn all(&self) -> Vec<f64> {
        let mut all = [self.app.as_slice(), self.timers.as_slice()].concat();
        all.sort_by(f64::total_cmp);
        all.dedup_by(|a, b| (*a - *b).abs() <= SAME_INSTANT);
        all
    }

    #[cfg(test)]
    pub(crate) fn app_len(&self) -> usize {
        self.app.len()
    }

    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.app.clear();
        self.timers.clear();
    }
}

fn insert(list: &mut Vec<f64>, deadline: f64) {
    let index = list.partition_point(|t| *t < deadline);
    let near = |i: usize| list.get(i).is_some_and(|t| (t - deadline).abs() <= SAME_INSTANT);
    if near(index) || index.checked_sub(1).is_some_and(near) {
        return;
    }
    list.insert(index, deadline);
}
