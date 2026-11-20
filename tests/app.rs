//! The apps of this crate plug into the core: they fit together with every
//! other app, show in the switcher, and their screens are served.

use std::sync::Arc;

use rust_web_server::test_client::TestClient;
use serde_json::Value;

#[test]
fn the_apps_fit_and_are_listed() {
    let apps = ortus_tasks::apps();
    ortus_core::app::check(&apps).unwrap();
    let instance = ortus_core::instance::testing::instance(apps);
    let client = TestClient::new(ortus_core::app(Arc::clone(&instance)));
    let body: Value = serde_json::from_str(client.get("/api/v1/apps").send().body_text()).unwrap();
    let ids: Vec<&str> = body["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    for id in ["tasks", "approvals"] {
        assert!(ids.contains(&id), "{id} is not listed: {ids:?}");
        assert_eq!(200, client.get(&format!("/{id}")).send().status());
    }
}
