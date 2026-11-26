//! The share sheet on the web: `navigator.share`, where the browser has it. The browser shares
//! only from a click or key press a moment ago, and shares no file by its path.

use js_sys::{Function, Object, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};

use super::{ShareError, Shown};

pub(crate) fn show(shown: Shown) {
    let Shown { share, reply, .. } = shown;
    if !share.files.is_empty() {
        reply.send(Err(ShareError::Unsupported("the web can't share a file by its path".into())));
        return;
    }
    let navigator = Reflect::get(&js_sys::global(), &"navigator".into()).unwrap_or(JsValue::UNDEFINED);
    let Ok(share_fn) = Reflect::get(&navigator, &"share".into()).and_then(|f| f.dyn_into::<Function>().map_err(JsValue::from)) else {
        reply.send(Err(ShareError::Unsupported("this browser has no navigator.share".into())));
        return;
    };
    let data = Object::new();
    for (key, value) in [("title", &share.title), ("text", &share.text), ("url", &share.url)] {
        if let Some(value) = value { Reflect::set(&data, &key.into(), &value.into()).ok(); }
    }
    let promise = match share_fn.call1(&navigator, &data).map(Promise::from) {
        Ok(promise) => promise,
        Err(error) => return reply.send(Err(ShareError::Failed(describe(&error)))),
    };
    wasm_bindgen_futures::spawn_local(async move {
        let result = match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(_) => Ok(Some("navigator.share".into())),
            Err(error) if name(&error).as_deref() == Some("AbortError") => Ok(None),
            Err(error) => Err(ShareError::Failed(describe(&error))),
        };
        reply.send(result);
    });
}

fn name(error: &JsValue) -> Option<String> { Reflect::get(error, &"name".into()).ok()?.as_string() }

fn describe(error: &JsValue) -> String {
    Reflect::get(error, &"message".into()).ok().and_then(|m| m.as_string()).or_else(|| name(error)).unwrap_or_else(|| format!("{error:?}"))
}
