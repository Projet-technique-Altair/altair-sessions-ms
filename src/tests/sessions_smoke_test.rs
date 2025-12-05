#[tokio::test]
async fn get_sessions_works() {
    let resp = reqwest::get("http://localhost:3003/sessions")
        .await
        .unwrap();
    assert!(resp.status().is_success());
}
