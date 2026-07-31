use emet_lsp::{completion_at, definition_at, hover_at};
use lsp_types::{Position, Uri};

fn scratch_uri() -> Uri {
    "untitled:scratch.emet".parse().unwrap()
}

const LET_STRING: &str = "main : List Scroll\nmain =\n  let greeting = \"hi\"\n  in []\n";

#[test]
fn hover_returns_inferred_type() {
    let hover = hover_at(&scratch_uri(), LET_STRING, Position::new(2, 6))
        .expect("hover at greeting binder");
    let text = match hover.contents {
        lsp_types::HoverContents::Markup(m) => m.value,
        _ => panic!("expected markup contents"),
    };
    assert!(text.contains("String"), "hover text: {text}");
}

#[test]
fn completion_returns_in_scope_names() {
    let items = completion_at(&scratch_uri(), LET_STRING, Position::new(3, 5));
    assert!(
        items.iter().any(|i| i.label == "greeting"),
        "completion includes local binding"
    );
    assert!(
        items.iter().any(|i| i.label == "main"),
        "completion includes sibling decl"
    );
}

#[test]
fn definition_returns_def_location_same_file() {
    let src = "greeting : String\ngreeting = \"hi\"\nmain : List Scroll\nmain =\n  let _unused = greeting\n  in []\n";
    let use_line = 4;
    let use_col = src.lines().nth(4).unwrap().find("greeting").unwrap() as u32;
    let uri: lsp_types::Uri = "file:///m.emet".parse().unwrap();
    let location =
        definition_at(&uri, src, Position::new(use_line, use_col)).expect("definition location");
    assert_eq!(location.uri, uri);
    assert_eq!(location.range.start.line, 1);
}
