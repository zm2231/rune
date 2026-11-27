//! A menu with a submenu, for checking the submenu from assistive technology: `App` opens a
//! menu whose `Process` row opens `Restart` and `Stop`. Prints `SUBMENU open=<bool>` when the
//! submenu opens or closes and `PICK <item>` when an item is picked.
use rune::{AppSpec, Cx, El, col};
use rune_kit::{Button, Kit, Menu, MenuEntry, MenuItem, root};

#[derive(Default)]
struct S {
    menu_open: bool,
    process_open: bool,
}

fn pick(name: &'static str) -> impl Fn(&mut S) {
    move |s: &mut S| {
        println!("PICK {name}");
        s.menu_open = false;
    }
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    let k = Kit::neutral(true);
    let mut layer = Vec::new();
    let process = MenuItem::new("Process")
        .key("process")
        .submenu(
            vec![
                MenuEntry::item(MenuItem::new("Restart").key("restart").on_select(pick("restart"))),
                MenuEntry::item(MenuItem::new("Stop").key("stop").on_select(pick("stop"))),
            ],
            s.process_open,
        )
        .on_submenu(|s: &mut S, open| {
            println!("SUBMENU open={open}");
            s.process_open = open;
        });
    let trigger = Menu::new("app")
        .trigger(Button::new("App").on_press(|s: &mut S| s.menu_open = !s.menu_open).build(&k))
        .item(MenuItem::new("New").key("new").on_select(pick("new")))
        .item(process)
        .open(s.menu_open)
        .on_close(|s: &mut S| s.menu_open = false)
        .build(&k, &mut layer);
    root(col().w_full().h_full().padding(16.0).fill(k.c.page).child(trigger), layer)
}

fn main() {
    rune::run(AppSpec::new("Submenu probe", (420.0, 260.0), || S { menu_open: true, ..S::default() }, view));
}
