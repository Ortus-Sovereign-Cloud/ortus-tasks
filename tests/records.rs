//! Records on both shapes: a signed PDF filed to a case through the
//! Records service gets a reference and is kept unchanged; only its
//! readers and administrators open it; its signatures check out; and
//! must-read policies are published, confirmed and counted.

mod common;

use std::sync::Arc;

use ortus_core::auth::accounts::Role;
use ortus_core::instance::testing_auth::signed_in;
use ortus_core::interfaces::{NewRecord, Records, Signing};
use rust_web_server::test_client::TestClient;
use serde_json::{json, Value};

fn call<A: rust_web_server::application::Application>(
    client: &TestClient<A>,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (i16, Value, Vec<u8>) {
    let url = format!("/api/v1{path}");
    let mut r = if method == "GET" {
        client.get(&url)
    } else {
        client.post(&url)
    };
    r = r
        .header("Authorization", &format!("Bearer {token}"))
        .header("X-Requested-With", "ortus");
    if let Some(b) = body {
        r = r
            .header("Content-Type", "application/json")
            .body_bytes(b.to_string().into_bytes());
    }
    let response = r.send();
    let bytes = response.body_bytes().to_vec();
    (
        response.status(),
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        bytes,
    )
}

#[test]
fn records_are_filed_kept_checked_and_policies_confirmed() {
    for (name, instance, _keep) in common::installations() {
        for app in instance.apps.clone() {
            app.provide(&instance, &instance.services);
        }
        let client = TestClient::new(ortus_core::app(Arc::clone(&instance)));
        let owner = signed_in(&instance, "olena@ministry.example", Role::Owner);
        let marek = signed_in(&instance, "marek@ministry.example", Role::Member);
        let sam = signed_in(&instance, "sam@ministry.example", Role::Member);
        let iryna = signed_in(&instance, "iryna@ministry.example", Role::Member);
        let id_of = |e: &str| {
            ortus_core::runtime::block_on(ortus_core::auth::accounts::by_email(
                instance.store.as_ref(),
                e,
            ))
            .unwrap()
            .unwrap()
            .id
        };

        // Marek signs a PDF; it is filed to a case with him as its reader.
        let signer =
            ortus_core::runtime::block_on(instance.services.get::<dyn Signing>(&instance.switches))
                .unwrap();
        let pdf = ortus_core::runtime::block_on(signer.sign(
            &id_of("marek@ministry.example"),
            &common::tiny_pdf("Flood response plan 2026"),
            "Author signs",
        ))
        .unwrap();
        let records =
            ortus_core::runtime::block_on(instance.services.get::<dyn Records>(&instance.switches))
                .unwrap();
        let readers = vec![id_of("marek@ministry.example")];
        let filed = ortus_core::runtime::block_on(records.file(NewRecord {
            by: &id_of("marek@ministry.example"),
            title: "Flood response plan 2026",
            kind: "decision",
            case: Some("2026/em/0114"),
            label: Some("official-sensitive"),
            pdf: &pdf,
            source: "/docs?doc=d1",
            readers: &readers,
        }))
        .unwrap();
        let second = ortus_core::runtime::block_on(records.file(NewRecord {
            by: &id_of("marek@ministry.example"),
            title: "Annex",
            kind: "decision",
            case: Some("2026/EM/0114"),
            label: None,
            pdf: &pdf,
            source: "/docs?doc=d2",
            readers: &readers,
        }))
        .unwrap();

        let (status, r, _) = call(
            &client,
            &marek,
            "GET",
            &format!("/records/records/{}", filed.id),
            None,
        );
        assert_eq!(200, status, "{name}: {r}");
        assert_eq!(
            (json!("0114-001"), json!("2026/EM/0114"), json!("decision")),
            (r["reference"].clone(), r["case"].clone(), r["kind"].clone()),
            "{name}: {r}"
        );
        let ten_years = r["keepUntil"].as_i64().unwrap() - r["filedAt"].as_i64().unwrap();
        assert!(ten_years >= 10 * 365 * 86_400_000, "{name}");
        let (_, r2, _) = call(
            &client,
            &marek,
            "GET",
            &format!("/records/records/{}", second.id),
            None,
        );
        assert_eq!(json!("0114-002"), r2["reference"], "{name}");
        // Strangers do not see it; administrators do.
        assert_eq!(
            404,
            call(
                &client,
                &sam,
                "GET",
                &format!("/records/records/{}", filed.id),
                None
            )
            .0,
            "{name}"
        );
        assert_eq!(
            404,
            call(
                &client,
                &sam,
                "GET",
                &format!("/records/records/{}/pdf", filed.id),
                None
            )
            .0,
            "{name}"
        );
        assert_eq!(
            200,
            call(
                &client,
                &owner,
                "GET",
                &format!("/records/records/{}", filed.id),
                None
            )
            .0,
            "{name}"
        );
        let (_, sams, _) = call(&client, &sam, "GET", "/records/records", None);
        assert_eq!(0, sams["records"].as_array().unwrap().len(), "{name}");

        // The file is the one filed, and its signature checks out.
        let (status, _, bytes) = call(
            &client,
            &marek,
            "GET",
            &format!("/records/records/{}/pdf", filed.id),
            None,
        );
        assert_eq!((200, true), (status, bytes == pdf), "{name}");
        let (_, checked, _) = call(
            &client,
            &marek,
            "GET",
            &format!("/records/records/{}/verify", filed.id),
            None,
        );
        assert_eq!(
            (json!(true), json!(true), json!(true)),
            (
                checked["unchanged"].clone(),
                checked["signatures"][0]["intact"].clone(),
                checked["signatures"][0]["trusted"].clone()
            ),
            "{name}: {checked}"
        );
        // Nothing changes or deletes a record: there is no route for it.
        let (status, _, _) = call(
            &client,
            &owner,
            "POST",
            &format!("/records/records/{}", filed.id),
            Some(json!({"title": "x"})),
        );
        assert!(status == 404 || status == 405, "{name}: {status}");

        // Must-read: an administrator publishes two policies to everyone.
        let body = json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Three rules cover most of your day."}]}]});
        assert_eq!(
            403,
            call(
                &client,
                &marek,
                "POST",
                "/records/must-read",
                Some(json!({"title": "x", "body": body}))
            )
            .0,
            "{name}"
        );
        let (status, p1, _) = call(
            &client,
            &owner,
            "POST",
            "/records/must-read",
            Some(
                json!({"title": "Data protection policy", "note": "Handbook · owner: Data protection officer", "body": body, "source": "/docs?doc=dp"}),
            ),
        );
        assert_eq!(200, status, "{name}: {p1}");
        call(
            &client,
            &owner,
            "POST",
            "/records/must-read",
            Some(json!({"title": "Flood response plan 2026", "body": body, "record": filed.id})),
        );
        // A third for a group Iryna is not in.
        call(
            &client,
            &owner,
            "POST",
            "/records/must-read",
            Some(json!({"title": "Pilots only", "body": body, "groups": ["Pilots"]})),
        );
        let (_, mine, _) = call(&client, &iryna, "GET", "/records/must-read", None);
        let titles: Vec<&str> = mine["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap())
            .collect();
        assert_eq!(
            vec!["Data protection policy", "Flood response plan 2026"],
            titles,
            "{name}"
        );
        let first = mine["items"][0]["id"].as_str().unwrap().to_string();
        let (status, after, _) = call(
            &client,
            &iryna,
            "POST",
            &format!("/records/must-read/{first}/confirm"),
            None,
        );
        assert!(
            status == 200 && after["items"][0]["confirmedAt"].is_i64(),
            "{name}: {after}"
        );
        // Confirming twice is once.
        call(
            &client,
            &iryna,
            "POST",
            &format!("/records/must-read/{first}/confirm"),
            None,
        );
        let (_, who, _) = call(
            &client,
            &owner,
            "GET",
            &format!("/records/must-read/{first}/confirmations"),
            None,
        );
        assert_eq!(
            1,
            who["confirmed"].as_array().unwrap().len(),
            "{name}: {who}"
        );
        assert_eq!(
            404,
            call(
                &client,
                &marek,
                "GET",
                &format!("/records/must-read/{first}/confirmations"),
                None
            )
            .0,
            "{name}"
        );
        // The record shows it was published as must-read.
        let (_, r, _) = call(
            &client,
            &marek,
            "GET",
            &format!("/records/records/{}", filed.id),
            None,
        );
        assert!(
            r["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "must-read"),
            "{name}: {r}"
        );
    }
}
