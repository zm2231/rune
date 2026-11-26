use serde_json::Value;

use super::*;

const CONTROLS: [&str; 20] = [
    "button", "default-button", "menu-item", "tab", "list-box-option", "tree-item", "combo-box", "radio-button",
    "menu-item-radio", "splitter", "check-box", "switch", "menu-item-check-box", "link", "slider", "spin-button",
    "search-input", "text-input", "multiline-text-input", "password-input",
];
const TEXTS: [&str; 11] = ["label", "paragraph", "status", "alert", "tooltip", "note", "heading", "image", "progress-indicator", "meter", "timer"];

struct Stop<'a> {
    node: &'a Value,
    frame: [f64; 4],
}

fn has(list: &Value, item: &str) -> bool {
    list.as_array().is_some_and(|l| l.iter().any(|v| v == item))
}

fn stops<'a>(node: &'a Value, clip: Option<[f64; 4]>, root: bool, out: &mut Vec<Stop<'a>>) {
    let role = node["role"].as_str().unwrap_or_default();
    let named = node["name"].as_str().is_some_and(|n| !n.trim().is_empty()) || node.get("range").is_some();
    let bounds = node["bounds"].as_array().map(|b| {
        let b: Vec<f64> = b.iter().map(|v| v.as_f64().unwrap_or_default()).collect();
        [b[0], b[1], b[0] + b[2], b[1] + b[3]]
    });
    let shown = bounds.map(|b| clip.map_or(b, |c| {
        let hold = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi.max(lo));
        [hold(b[0], c[0], c[2]), hold(b[1], c[1], c[3]), hold(b[2], c[0], c[2]), hold(b[3], c[1], c[3])]
    }));
    if let Some(s) = shown && (CONTROLS.contains(&role) || has(&node["actions"], "click") || (TEXTS.contains(&role) && named)) {
        out.push(Stop { node, frame: [s[0], s[1], s[2] - s[0], s[3] - s[1]] });
        return;
    }
    let clip = if has(&node["states"], "clips") && !root { shown.or(clip) } else { clip };
    for child in node["children"].as_array().into_iter().flatten() { stops(child, clip, false, out); }
}

fn traits_for(node: &Value) -> Vec<&'static str> {
    let role = node["role"].as_str().unwrap_or_default();
    let mut want = Vec::new();
    match role {
        "check-box" | "switch" | "menu-item-check-box" => want.extend(["button", "toggle"]),
        "link" => want.push("link"),
        "slider" | "spin-button" => want.push("adjustable"),
        "heading" => want.extend(["static-text", "header"]),
        "search-input" => want.push("search"),
        r if CONTROLS[..10].contains(&r) || has(&node["actions"], "click") && !role.ends_with("input") => want.push("button"),
        _ => {}
    }
    if has(&node["states"], "disabled") { want.push("disabled"); }
    if has(&node["states"], "selected") { want.push("selected"); }
    want
}

fn compare(story: &str, dump: &Value, exported: &Value) -> Vec<String> {
    let mut want = Vec::new();
    stops(dump, None, true, &mut want);
    let got = exported.as_array().cloned().unwrap_or_default();
    let mut wrong = Vec::new();
    if want.len() != got.len() {
        wrong.push(format!("{story}: {} stops in the dump, {} elements exported", want.len(), got.len()));
    }
    for (stop, element) in want.iter().zip(&got) {
        let n = stop.node;
        let name = n["name"].as_str().or(n["placeholder"].as_str()).unwrap_or_default();
        let at = format!("{story} {}", n["key"].as_str().unwrap_or(name));
        if element["label"] != name { wrong.push(format!("{at}: label {} for name {name:?}", element["label"])); }
        let frame: Vec<f64> = element["frame"].as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
        if frame.len() != 4 || frame.iter().zip(stop.frame).any(|(a, b)| (a - b).abs() > 0.11) {
            wrong.push(format!("{at}: frame {frame:?} for bounds {:?}", stop.frame));
        }
        for t in traits_for(n) {
            if !has(&element["traits"], t) { wrong.push(format!("{at}: no {t} trait in {}", element["traits"])); }
        }
        let activates = has(&n["actions"], "click") || has(&n["actions"], "focus");
        if activates != has(&element["actions"], "activate") { wrong.push(format!("{at}: activate is {} for {}", !activates, n["actions"])); }
        for (action, gesture) in [("increment", "increment"), ("decrement", "decrement"), ("context-menu", "menu")] {
            if has(&n["actions"], action) != has(&element["actions"], gesture) {
                wrong.push(format!("{at}: {action} in the dump but {gesture} in {}", element["actions"]));
            }
        }
        let checked = if has(&n["states"], "checked") { Some("1") } else if has(&n["states"], "unchecked") { Some("0") } else { None };
        if let Some(v) = checked && element["value"] != v { wrong.push(format!("{at}: value {} when {v}", element["value"])); }
        let expanded = if has(&n["states"], "expanded") { Some(true) } else if has(&n["states"], "collapsed") { Some(false) } else { None };
        if element["expanded"].as_bool() != expanded { wrong.push(format!("{at}: expanded {} for {expanded:?}", element["expanded"])); }
    }
    wrong
}

#[test]
fn every_storys_ios_elements_match_its_accessibility_tree() {
    let mut wrong = Vec::new();
    let mut controls = 0;
    for (id, setup) in stories::SCENES {
        let mut rt = build_app().headless_runtime((720.0, 480.0), rune::Env::new(true, 2.0));
        rt.update(|s, _| setup(s));
        for _ in 0..3 { rt.frame(1.0 / 60.0); }
        let exported = rt.ios_accessibility_json();
        controls += exported.as_array().map_or(0, |a| a.iter().filter(|e| has(&e["actions"], "activate")).count());
        wrong.extend(compare(id, &rt.accessibility_json(), &exported));
    }
    assert!(controls > 100, "the stories hold the gallery's controls: {controls}");
    assert!(wrong.is_empty(), "{} mismatches:\n{}", wrong.len(), wrong.join("\n"));
}
