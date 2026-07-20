use super::*;
use wiremock::matchers::{body_bytes, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn propfind_parser_accepts_namespaces_absolute_urls_and_encoded_names() {
    let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:">
          <d:response><d:href>https://dav.test/root/changes/</d:href></d:response>
          <d:response><d:href>/root/changes/device%20one/</d:href></d:response>
        </d:multistatus>"#;
    assert_eq!(
        parse_propfind_names(xml, "changes").unwrap(),
        vec!["device one".to_string()]
    );
}

#[tokio::test]
async fn prepare_creates_yjs_head_and_update_collections() {
    let server = MockServer::start().await;
    for collection_path in [
        "/",
        "/state",
        "/state/device-a",
        "/state/device-a/note",
        "/state/device-a/clipboard",
        "/state/device-a/attachment",
        "/state/device-a/tag",
        "/state/device-a/note_tag",
        "/state/device-a/meta",
        "/heads",
        "/heads/notes",
        "/yjs",
        "/yjs/snapshots",
        "/yjs/updates",
        "/attachments",
    ] {
        Mock::given(method("MKCOL"))
            .and(path(collection_path))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&server)
            .await;
    }

    let provider = WebDavProvider::new(&server.uri(), "user", "pass").unwrap();
    provider.prepare("device-a").await.unwrap();
}

#[tokio::test]
async fn list_accepts_webdav_207_multistatus() {
    let server = MockServer::start().await;
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response><d:href>/root/changes/</d:href></d:response>
      <d:response><d:href>/root/changes/device-a/</d:href></d:response>
    </d:multistatus>"#;
    Mock::given(method("PROPFIND"))
        .and(path("/root/changes"))
        .respond_with(ResponseTemplate::new(207).set_body_string(xml))
        .mount(&server)
        .await;

    let provider = WebDavProvider::new(&format!("{}/root", server.uri()), "user", "pass").unwrap();
    assert_eq!(provider.list("changes").await.unwrap(), vec!["device-a"]);
}

#[tokio::test]
async fn immutable_put_accepts_identical_precondition_retry() {
    let server = MockServer::start().await;
    let body = br#"{"schema_version":2}"#.to_vec();
    let change_path = "/root/attachments/abc123";
    Mock::given(method("PUT"))
        .and(path(change_path))
        .and(header("if-none-match", "*"))
        .and(body_bytes(body.clone()))
        .respond_with(ResponseTemplate::new(412))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(change_path))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let provider = WebDavProvider::new(&format!("{}/root", server.uri()), "user", "pass").unwrap();
    provider
        .put("attachments/abc123", body, "application/octet-stream")
        .await
        .unwrap();
}

#[tokio::test]
async fn slow_webdav_response_times_out() {
    let server = MockServer::start().await;
    Mock::given(method("PROPFIND"))
        .and(path("/root/changes"))
        .respond_with(
            ResponseTemplate::new(207)
                .set_delay(Duration::from_millis(100))
                .set_body_string("<d:multistatus xmlns:d=\"DAV:\"/>"),
        )
        .mount(&server)
        .await;
    let provider = WebDavProvider::new_with_timeout(
        &format!("{}/root", server.uri()),
        "user",
        "pass",
        Duration::from_millis(20),
    )
    .unwrap();
    assert!(provider.list("changes").await.is_err());
}

#[tokio::test]
#[ignore = "set QUICKNOTE_WEBDAV_TEST_URL to run against a live WebDAV server"]
async fn live_webdav_smoke_test() {
    let endpoint =
        std::env::var("QUICKNOTE_WEBDAV_TEST_URL").expect("QUICKNOTE_WEBDAV_TEST_URL is required");
    let username = std::env::var("QUICKNOTE_WEBDAV_TEST_USERNAME").unwrap_or_default();
    let password = std::env::var("QUICKNOTE_WEBDAV_TEST_PASSWORD").unwrap_or_default();
    let device_id = format!("smoke-{}", uuid::Uuid::new_v4());
    let provider = WebDavProvider::new(&endpoint, &username, &password).unwrap();
    provider.prepare(&device_id).await.unwrap();

    let path = format!("state/{device_id}/note/test-note.json");
    let body = br#"{"schema_version":2,"smoke":true}"#.to_vec();
    provider
        .put(&path, body.clone(), "application/json")
        .await
        .unwrap();
    assert_eq!(provider.get(&path).await.unwrap(), Some(body));
    assert!(provider
        .list(&format!("state/{device_id}/note"))
        .await
        .unwrap()
        .contains(&"test-note.json".to_string()));
}
