use crate::open_url::incoming::{Glue, Inbox};
use crate::{Activity, App, AppSpec, Cx, El, Env, Runtime, Tx, el};

struct FakeGlue;

impl Glue for FakeGlue {
    fn handled_elsewhere(&self) -> bool { false }
    fn install(&self) -> bool { true }
    fn remove(&self) {}
}

fn blank(_: &Vec<Activity>, _: &mut Cx<Vec<Activity>>) -> El { el().size(20.0, 20.0) }

#[test]
fn an_activity_that_launched_the_app_and_a_later_one_reach_the_hook_in_order() {
    let inbox: &'static Inbox<Activity> = Box::leak(Box::new(Inbox::new()));
    inbox.prepare(&FakeGlue);
    let launched = Activity::new("com.example.notes.editing").with_info("note", "42");
    inbox.receive(launched.clone());

    let app = App::new(AppSpec::new("handoff", (80.0, 60.0), Vec::new, blank)).on_continue_activity(|seen, activity, _| seen.push(activity.clone()));
    let _listening = super::listen_in(inbox, &app, &FakeGlue).expect("on_continue_activity listens");
    let later = Activity::new("com.example.notes.reading").with_title("Groceries");
    inbox.receive(later.clone());
    let mut rt = Runtime::new(Vec::new(), blank, (80.0, 60.0), Env::new(true, 2.0));
    for f in app.shared.take_updates() { rt.update_if(f); }
    assert_eq!(*rt.state, [launched, later]);
}

#[test]
fn the_last_advertise_in_a_frame_is_what_the_window_gets_and_repeating_it_changes_nothing() {
    fn view(_: &u32, _: &mut Cx<u32>) -> El {
        el().key("advertise").size(40.0, 40.0).on_press_tx(|n: &mut u32, tx: &mut Tx| {
            *n += 1;
            tx.advertise(Activity::new("com.example.first"));
            tx.advertise(Activity::new("com.example.notes.editing").with_info("note", "42"));
        })
    }
    let mut rt = Runtime::new(0u32, view, (80.0, 60.0), Env::new(true, 2.0));
    rt.frame(0.0);
    let press = |rt: &mut Runtime<u32>, t: f64| { rt.pointer_down(10.0, 10.0); rt.pointer_up(10.0, 10.0); rt.frame(t); };
    press(&mut rt, 1.0 / 60.0);
    let host = &mut rt.core_mut().host;
    assert_eq!(host.activity.take_change().cloned(), Some(Some(Activity::new("com.example.notes.editing").with_info("note", "42"))));
    press(&mut rt, 2.0 / 60.0);
    assert_eq!(rt.core_mut().host.activity.take_change(), None, "the window already has it");
}
