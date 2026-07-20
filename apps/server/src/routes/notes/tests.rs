use super::{collect_attachment_candidates, extract_title};

#[test]
fn title_is_plain_text_and_bounded() {
    assert_eq!(
        extract_title("<h1>Hello <strong>cloud</strong></h1>"),
        "Hello cloud"
    );
    assert_eq!(
        extract_title("<p>标题</p><p>正文第一行</p><p>正文第二行</p>"),
        "标题"
    );
    assert_eq!(extract_title("<p></p>"), "Untitled");
    assert_eq!(extract_title(&"x".repeat(120)).chars().count(), 100);
}

#[test]
fn extracts_unique_attachment_ids() {
    let ids = collect_attachment_candidates([
        r#"<p><img src="attachment://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"></p>"#,
        r#"<p><img src="attachment://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"></p>"#,
        r#"<p><img src="attachment://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"></p>"#,
    ]);
    assert_eq!(
        ids,
        vec![
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        ]
    );
}
