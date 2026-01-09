#[tokio::test]
async fn health_check_works() {
    let resp = reqwest::get("http://localhost:3003/health")
        .await
        .unwrap();
    assert!(resp.status().is_success());
}
