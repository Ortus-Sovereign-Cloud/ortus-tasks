//! The slice 7 review's findings, as tests: record references unique
//! across cases and years; task links stay on this site; the PDF reader
//! neither hangs nor panics on broken input; leaving wipes one's key.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use ortus_core::auth::accounts::Role;
use ortus_core::instance::testing_auth::signed_in;
use ortus_core::interfaces::{NewRecord, Records, Signing};
use ortus_tasks::pades;
use rust_web_server::test_client::TestClient;
use serde_json::{json, Value};

#[test]
fn references_links_parser_and_leaving() {
    for (name, instance, _keep) in common::installations() {
        for app in instance.apps.clone() {
            app.provide(&instance, &instance.services);
        }
        let client = TestClient::new(ortus_core::app(Arc::clone(&instance)));
        let marek = signed_in(&instance, "marek@ministry.example", Role::Member);
        let id = ortus_core::runtime::block_on(ortus_core::auth::accounts::by_email(
            instance.store.as_ref(),
            "marek@ministry.example",
        ))
        .unwrap()
        .unwrap()
        .id;
        let records =
            ortus_core::runtime::block_on(instance.services.get::<dyn Records>(&instance.switches))
                .unwrap();
        let pdf = common::tiny_pdf("x");
        let readers = vec![id.clone()];
        // Same last part, different cases (and years): both are filed.
        for case in ["2026/GEN", "2027/GEN", "2026/EM/0114", "2026/HR/0114"] {
            let r = ortus_core::runtime::block_on(records.file(NewRecord {
                by: &id,
                title: case,
                kind: "decision",
                case: Some(case),
                label: None,
                pdf: &pdf,
                source: "/docs",
                readers: &readers,
                channel: None,
            }));
            assert!(r.is_ok(), "{name}: {case}: {r:?}");
        }
        let response = client
            .get("/api/v1/records/records")
            .header("Authorization", &format!("Bearer {marek}"))
            .header("X-Requested-With", "ortus")
            .send();
        let list: Value = serde_json::from_slice(response.body_bytes()).unwrap();
        let refs: Vec<&str> = list["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["reference"].as_str().unwrap())
            .collect();
        assert_eq!(
            2,
            refs.iter().filter(|r| **r == "GEN-001").count(),
            "{name}: {refs:?}"
        );

        // A task's link to another site is dropped.
        let post = |path: &str, v: Value| {
            let r = client
                .post(&format!("/api/v1{path}"))
                .header("Authorization", &format!("Bearer {marek}"))
                .header("X-Requested-With", "ortus")
                .header("Content-Type", "application/json")
                .body_bytes(v.to_string().into_bytes())
                .send();
            serde_json::from_slice::<Value>(r.body_bytes()).unwrap_or(Value::Null)
        };
        let p = post("/tasks/projects", json!({"name": "P"}));
        for bad in ["//evil.example/x", "/\\evil.example"] {
            let t = post(
                "/tasks/tasks",
                json!({"project": p["id"], "title": "t", "source": bad}),
            );
            assert!(t["source"].is_null(), "{name}: {bad}: {t}");
        }
        let t = post(
            "/tasks/tasks",
            json!({"project": p["id"], "title": "t", "source": "/chat?room=c1"}),
        );
        assert_eq!(json!("/chat?room=c1"), t["source"], "{name}");

        // Broken PDFs: an answer, quickly, never a hang or a panic.
        let started = Instant::now();
        let signer = ortus_core::runtime::block_on(ortus_tasks::certs::personal(
            &instance,
            &ortus_core::runtime::block_on(ortus_core::auth::accounts::by_id(
                instance.store.as_ref(),
                &id,
            ))
            .unwrap()
            .unwrap(),
        ))
        .unwrap();
        for broken in [
            &b"<</K[>]>>"[..],
            b"%PDF-1.7\nxref\n0 1\ntrailer\n<</Root [>] >>\nstartxref\n9\n%%EOF",
            b"%PDF /ByteRange [0 5 6 1] <>",
            b"/ByteRange [0 3 4 0]<>",
            b"",
        ] {
            assert!(
                pades::sign(
                    broken,
                    &signer,
                    &[],
                    &pades::Signing {
                        name: "x",
                        reason: "x",
                        at_ms: 0
                    }
                )
                .is_err(),
                "{name}"
            );
            let _ = pades::verify(broken, &signer.cert);
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{name}: the reader hung"
        );

        // Leaving: the signing key is wiped (its certificate stays, for checking).
        let signing =
            ortus_core::runtime::block_on(instance.services.get::<dyn Signing>(&instance.switches))
                .unwrap();
        ortus_core::runtime::block_on(signing.sign(&id, &pdf, "x")).unwrap();
        ortus_core::runtime::block_on(async {
            let mut tx = instance.store.begin().await.unwrap();
            for app in instance.apps.iter() {
                app.forget_user(tx.as_mut(), &id, &ortus_core::retention::Rules::default())
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
        });
        let keys = ortus_core::runtime::block_on(
            instance
                .store
                .query("SELECT key_sealed, revoked_at FROM signing_keys", &[]),
        )
        .unwrap();
        assert!(
            keys.iter()
                .all(|r| r.text(0) == Some("") && r.int(1).is_some()),
            "{name}"
        );
    }
}
