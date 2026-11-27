use crate::open_url::incoming::{Glue, Inbox};
use crate::{App, AppSpec, Env, Runtime, ServiceInput, el};

struct FakeGlue;

impl Glue for FakeGlue {
    fn handled_elsewhere(&self) -> bool { false }
    fn install(&self) -> bool { true }
    fn remove(&self) {}
}

fn app() -> App<Vec<ServiceInput>> {
    let spec = AppSpec::new("services", (80.0, 60.0), Vec::new, |_: &Vec<ServiceInput>, _: &mut crate::Cx<Vec<ServiceInput>>| el().size(20.0, 20.0));
    App::new(spec).on_service(|seen, input, _| seen.push(input.clone()))
}

#[test]
fn a_service_that_launched_the_app_and_a_later_one_reach_the_hook_in_order() {
    let inbox: &'static Inbox<ServiceInput> = Box::leak(Box::new(Inbox::new()));
    inbox.prepare(&FakeGlue);
    let launched = ServiceInput::new("send").with_text("selected words");
    inbox.receive(launched.clone());

    let app = app();
    let _listening = super::listen_in(inbox, &app, &FakeGlue).expect("on_service listens");
    let later = ServiceInput::new("files").with_file("/notes.md").with_url("https://example.com/");
    inbox.receive(later.clone());
    let mut rt = Runtime::new(Vec::new(), |_: &Vec<ServiceInput>, _: &mut crate::Cx<Vec<ServiceInput>>| el(), (80.0, 60.0), Env::new(true, 2.0));
    for f in app.shared.take_updates() { rt.update_if(f); }
    assert_eq!(*rt.state, [launched, later]);
}
