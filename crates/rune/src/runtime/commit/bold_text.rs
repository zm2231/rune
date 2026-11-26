use crate::el::TextSpec;

pub(super) fn bolder(weight: f32) -> f32 {
    (weight + if weight < 600.0 { 200.0 } else { 100.0 }).min(900.0)
}

pub(super) fn embolden(text: &mut TextSpec) {
    text.style.weight = bolder(text.style.weight);
    for span in &mut text.spans {
        if let Some(weight) = &mut span.weight { *weight = bolder(*weight); }
    }
    text.shape = None;
}

#[cfg(test)]
#[path = "bold_text_tests.rs"]
mod tests;
