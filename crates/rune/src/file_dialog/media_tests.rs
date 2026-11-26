use std::path::PathBuf;

use crate::{Cx, El, Env, MediaKind, MediaPicker, Runtime, Tx, el};

#[derive(Default)]
struct Album {
    picker: MediaPicker,
    answers: Vec<Result<Option<Vec<PathBuf>>, String>>,
}

fn view(_: &Album, _: &mut Cx<Album>) -> El {
    el().key("pick").size(80.0, 20.0).on_press_tx(|s: &mut Album, tx: &mut Tx| {
        tx.pick_media(s.picker.clone(), |s: &mut Album, result, _| s.answers.push(result.map_err(|e| e.0)));
    })
}

fn asked(picker: MediaPicker) -> Runtime<Album> {
    let mut rt = Runtime::new(Album { picker, ..Default::default() }, view, (200.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.pointer_down(10.0, 10.0);
    rt.pointer_up(10.0, 10.0);
    rt.frame(1.0 / 60.0);
    rt
}

#[test]
fn one_photo_is_a_single_open_of_image_files_answered_as_a_list() {
    let mut rt = asked(MediaPicker::default());
    let json = rt.file_dialog_json().expect("the press asked for a picker");
    assert_eq!((json["kind"].as_str(), json["media"].as_str(), json.get("multiple")), (Some("open"), Some("images"), None));
    let filters = json["filters"].as_array().unwrap();
    assert_eq!(filters.len(), 1);
    assert_eq!(filters[0]["name"], "Images");
    assert!(filters[0]["extensions"].as_array().unwrap().iter().any(|e| e == "heic"));

    rt.resolve_file_dialog(Some("/photos/IMG_0001.HEIC".into())).unwrap();
    rt.frame(2.0 / 60.0);
    assert_eq!(rt.state.answers, [Ok(Some(vec![PathBuf::from("/photos/IMG_0001.HEIC")]))]);
}

#[test]
fn any_other_limit_picks_several_and_both_kinds_offer_both_and_each() {
    let mut rt = asked(MediaPicker::new(MediaKind::ImagesAndVideos).limit(0));
    let json = rt.file_dialog_json().unwrap();
    assert_eq!((json["media"].as_str(), json["multiple"].as_bool()), (Some("images-and-videos"), Some(true)));
    let names: Vec<_> = json["filters"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(names, ["Images and videos", "Images", "Videos"]);

    rt.resolve_file_dialog_files(None).unwrap();
    rt.frame(2.0 / 60.0);
    assert_eq!(rt.state.answers, [Ok(None)], "cancelling answers None");
}
