use crate::{Env, Runtime, TextEdit, TextStyle, col, text, text_input};

const HINT: &str = "Describe what changed, why it changed, and how a reviewer can check it";
const INDENT: f32 = 40.0;

fn style() -> TextStyle {
    TextStyle::new(13.0, 400.0).lh(18.0)
}

struct Field {
    width: f32,
}

fn view(s: &Field, _: &mut crate::Cx<Field>) -> crate::El {
    let field = text_input(&TextEdit::new(""), style()).text_indent(INDENT).placeholder(HINT, crate::Color::WHITE);
    col()
        .items_start()
        .child(field.key("field").w(s.width))
        .child(text(HINT).font(style()).wrap().key("painted").w(s.width - INDENT))
}

#[test]
fn an_indented_empty_field_holds_its_placeholder_as_painted() {
    let mut width = 120.0;
    while width <= 300.0 {
        let mut rt = Runtime::new(Field { width }, view, (600.0, 1200.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let h = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).expect(key).rect[3];
        assert_eq!(h("field"), h("painted"), "at {width} points wide");
        width += 3.0;
    }
}
