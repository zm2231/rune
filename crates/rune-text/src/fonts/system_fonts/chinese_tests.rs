use std::cell::Cell;
use std::path::PathBuf;

use super::apple_names::cjk_candidates;
use super::chinese::*;

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

/// A system's Chinese families Rune can draw and their files. Counts the families asked for.
struct System {
    families: &'static [(&'static str, &'static [&'static str])],
    lookups: Cell<usize>,
}

impl System {
    fn new(families: &'static [(&'static str, &'static [&'static str])]) -> Self {
        System { families, lookups: Cell::new(0) }
    }

    fn files_of(&self, name: &str) -> Vec<PathBuf> {
        self.lookups.set(self.lookups.get() + 1);
        self.families.iter().find(|(family, _)| *family == name).map(|(_, files)| paths(files)).unwrap_or_default()
    }

    fn family(&self, state: &mut ChineseFallbacks<u32>, pingfang: &str, locale: Option<&str>) -> Option<Vec<PathBuf>> {
        state.family(cjk_candidates(pingfang, locale), |name| self.files_of(name)).map(<[PathBuf]>::to_vec)
    }
}

const MAC: &[(&str, &[&str])] = &[("Hiragino Sans GB", &["Hiragino Sans GB.ttc"]), ("Heiti SC", &["STHeiti Light.ttc", "STHeiti Medium.ttc"]), ("Heiti TC", &["STHeiti Light.ttc", "STHeiti Medium.ttc"])];

#[test]
fn the_first_candidate_with_files_is_chosen() {
    let system = System::new(MAC);
    let mut state = ChineseFallbacks::default();
    assert_eq!(system.family(&mut state, ".PingFang UI SC", None), Some(paths(&["Hiragino Sans GB.ttc"])));
    assert_eq!(system.family(&mut state, ".PingFang UI SC", Some("zh-Hant")), Some(paths(&["STHeiti Light.ttc", "STHeiti Medium.ttc"])));
    // Without Hiragino Sans GB, Simplified takes the next candidate.
    let system = System::new(&[("Heiti SC", &["STHeiti Light.ttc"])]);
    assert_eq!(system.family(&mut ChineseFallbacks::default(), "PingFang SC", Some("zh-Hans")), Some(paths(&["STHeiti Light.ttc"])));
    // No candidate at all.
    let system = System::new(&[]);
    assert_eq!(system.family(&mut ChineseFallbacks::default(), "PingFang SC", None), None);
}

/// The second character, and every one after, asks for no family: whether one was found (one
/// lookup) or every candidate was tried and none had files (three).
#[test]
fn a_second_chinese_character_resolves_nothing_again() {
    for (families, lookups) in [(MAC, 1), (&[][..], 3)] {
        let system = System::new(families);
        let mut state = ChineseFallbacks::default();
        let first = system.family(&mut state, ".PingFang UI SC", Some("zh-Hans"));
        assert_eq!(system.lookups.get(), lookups);
        for locale in [Some("zh-Hans"), None, Some("zh-CN"), Some("en")] {
            assert_eq!(system.family(&mut state, ".PingFang UI SC", locale), first);
        }
        assert_eq!(system.lookups.get(), lookups, "asked again");
    }
}

#[test]
fn simplified_and_traditional_are_each_resolved_once() {
    let system = System::new(MAC);
    let mut state = ChineseFallbacks::default();
    system.family(&mut state, ".PingFang UI SC", None);
    system.family(&mut state, ".PingFang UI TC", None);
    assert_eq!(system.lookups.get(), 2);
    system.family(&mut state, ".PingFang UI SC", Some("zh-TW"));
    system.family(&mut state, "PingFang HK", None);
    assert_eq!(system.lookups.get(), 2);
}

#[test]
fn a_cascade_is_listed_once_per_locale_and_the_lists_kept_are_bounded() {
    let mut state = ChineseFallbacks::<u32>::default();
    let builds = Cell::new(0);
    let cascade = |state: &mut ChineseFallbacks<u32>, locale: Option<&str>, fonts: Vec<u32>| {
        state.cascade(locale, || { builds.set(builds.get() + 1); fonts })
    };
    assert_eq!(cascade(&mut state, None, vec![1, 2]), [1, 2]);
    assert_eq!(cascade(&mut state, None, vec![9]), [1, 2], "the list kept");
    assert_eq!(cascade(&mut state, Some("zh-Hant"), vec![3]), [3]);
    assert_eq!(cascade(&mut state, Some("zh-Hant"), vec![9]), [3]);
    assert_eq!(builds.get(), 2);
    // A list is cut to the bound.
    let long: Vec<u32> = (0..2 * MAX_CASCADE as u32).collect();
    assert_eq!(cascade(&mut state, Some("ja"), long).len(), MAX_CASCADE);
    // Past the bound on locales the oldest list (no locale's) goes.
    for i in 0..MAX_LOCALES - 3 {
        cascade(&mut state, Some(&format!("l{i}")), vec![0]);
    }
    assert_eq!(builds.get(), MAX_LOCALES);
    assert_eq!(cascade(&mut state, None, vec![7]), [1, 2], "still kept at the bound");
    cascade(&mut state, Some("one-more"), vec![0]);
    assert_eq!(cascade(&mut state, None, vec![7]), [7], "listed again");
}
