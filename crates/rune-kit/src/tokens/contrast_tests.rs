use crate::Kit;

fn ratio(ink: rune::Color, page: rune::Color) -> f32 {
    let ink = page.mix(ink.alpha(1.0), ink.a);
    let (a, b) = (ink.luminance() + 0.05, page.luminance() + 0.05);
    a.max(b) / a.min(b)
}

#[test]
fn increased_contrast_sets_quiet_text_and_borders_further_from_the_page_in_both_appearances() {
    for dark in [false, true] {
        let k = Kit::neutral(dark);
        let more = k.increased_contrast();
        let page = k.c.page;
        for (name, before, after) in [
            ("text_muted", k.c.text_muted, more.c.text_muted),
            ("text_faint", k.c.text_faint, more.c.text_faint),
            ("icon_muted", k.c.icon_muted, more.c.icon_muted),
            ("border_muted", k.c.border_muted, more.c.border_muted),
            ("border_base", k.c.border_base, more.c.border_base),
            ("border_strong", k.c.border_strong, more.c.border_strong),
        ] {
            assert!(ratio(after, page) > ratio(before, page), "{name} (dark {dark}): {} -> {}", ratio(before, page), ratio(after, page));
        }
        assert_eq!(more.c.text_base, k.c.text_base);
        assert_eq!(more.c.page, page);
    }
}

#[test]
fn asking_for_increased_contrast_twice_changes_nothing_more() {
    for dark in [false, true] {
        let once = Kit::neutral(dark).increased_contrast();
        let twice = once.increased_contrast();
        assert_eq!(format!("{:?}", twice.c), format!("{:?}", once.c), "palette (dark {dark})");
        assert_eq!(twice.roles, once.roles, "roles (dark {dark})");
        assert!(twice.increased_contrast);
    }
}
