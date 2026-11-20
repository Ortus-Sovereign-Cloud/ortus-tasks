//! Tasks on both shapes: a project and its members, tasks with who does
//! them, due times and checklists, moved across the board, one's own
//! list, search for members only, and forgetting a person.

mod common;

use std::sync::Arc;

use ortus_core::auth::accounts::Role;
use ortus_core::instance::testing_auth::signed_in;
use rust_web_server::test_client::TestClient;
use serde_json::{json, Value};

fn call<A: rust_web_server::application::Application>(
    client: &TestClient<A>,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (i16, Value) {
    let url = format!("/api/v1{path}");
    let mut r = match method {
        "GET" => client.get(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        _ => client.post(&url),
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
    (
        response.status(),
        serde_json::from_slice(response.body_bytes()).unwrap_or(Value::Null),
    )
}

#[test]
fn a_project_board_with_tasks_people_and_checklists() {
    for (name, instance, _keep) in common::installations() {
        let client = TestClient::new(ortus_core::app(Arc::clone(&instance)));
        let olena = signed_in(&instance, "olena@ministry.example", Role::Member);
        let marek = signed_in(&instance, "marek@ministry.example", Role::Member);
        let sam = signed_in(&instance, "sam@ministry.example", Role::Member);
        let id_of = |e: &str| {
            ortus_core::runtime::block_on(ortus_core::auth::accounts::by_email(
                instance.store.as_ref(),
                e,
            ))
            .unwrap()
            .unwrap()
            .id
        };

        // Olena's project; nobody else sees it yet.
        let (status, p) = call(
            &client,
            &olena,
            "POST",
            "/tasks/projects",
            Some(json!({"name": "Flood response 2026"})),
        );
        assert_eq!(200, status, "{name}: {p}");
        let project = p["id"].as_str().unwrap().to_string();
        assert_eq!(
            404,
            call(
                &client,
                &marek,
                "GET",
                &format!("/tasks/projects/{project}"),
                None
            )
            .0,
            "{name}"
        );
        assert_eq!(
            400,
            call(
                &client,
                &olena,
                "POST",
                "/tasks/projects",
                Some(json!({"name": "  "}))
            )
            .0,
            "{name}"
        );

        // A task for Marek with a due time and a checklist: he joins.
        let due = ortus_core::program::now_ms() + 3_600_000;
        let (status, t) = call(
            &client,
            &olena,
            "POST",
            "/tasks/tasks",
            Some(json!({
                "project": project, "title": "Close road R-12 and divert traffic",
                "notes": "Decided in #flood-response at 10:47.", "assignee": "marek@ministry.example",
                "dueAt": due, "checklist": ["Barriers at both ends", "Signs for the detour", "Tell the police", "Radio announcement"],
                "source": "/chat?message=m1",
            })),
        );
        assert_eq!(200, status, "{name}: {t}");
        let road = t["id"].as_str().unwrap().to_string();
        assert_eq!(
            (
                json!(id_of("marek@ministry.example")),
                json!(due),
                json!([0, 4])
            ),
            (
                t["assignee"].clone(),
                t["dueAt"].clone(),
                t["checked"].clone()
            ),
            "{name}: {t}"
        );
        assert_eq!(
            200,
            call(
                &client,
                &marek,
                "GET",
                &format!("/tasks/projects/{project}"),
                None
            )
            .0,
            "{name}"
        );
        // Not a colleague: refused.
        assert_eq!(
            400,
            call(
                &client,
                &olena,
                "POST",
                "/tasks/tasks",
                Some(json!({"project": project, "title": "x", "assignee": "nobody@else.example"}))
            )
            .0,
            "{name}"
        );
        // Sam is not a member: neither the project nor its tasks.
        assert_eq!(
            404,
            call(
                &client,
                &sam,
                "POST",
                "/tasks/tasks",
                Some(json!({"project": project, "title": "Mine now"}))
            )
            .0,
            "{name}"
        );
        assert_eq!(
            404,
            call(&client, &sam, "GET", &format!("/tasks/tasks/{road}"), None).0,
            "{name}"
        );
        assert_eq!(
            404,
            call(
                &client,
                &sam,
                "PATCH",
                &format!("/tasks/tasks/{road}"),
                Some(json!({"title": "Hijacked"}))
            )
            .0,
            "{name}"
        );

        // Two more cards; Marek ticks two items and starts the road task.
        let (_, buses) = call(
            &client,
            &olena,
            "POST",
            "/tasks/tasks",
            Some(json!({"project": project, "title": "Book 6 buses for evacuation"})),
        );
        let (_, sheet) = call(
            &client,
            &olena,
            "POST",
            "/tasks/tasks",
            Some(json!({"project": project, "title": "Shelter capacity sheet", "state": "done"})),
        );
        assert!(sheet["doneAt"].is_i64(), "{name}: {sheet}");
        for n in [0, 1] {
            let (status, _) = call(
                &client,
                &marek,
                "PATCH",
                &format!("/tasks/tasks/{road}/checks/{n}"),
                Some(json!({"done": true})),
            );
            assert_eq!(200, status, "{name}");
        }
        let (status, moved) = call(
            &client,
            &marek,
            "PATCH",
            &format!("/tasks/tasks/{road}"),
            Some(json!({"state": "doing"})),
        );
        assert_eq!(
            (200, json!("doing"), json!([2, 4])),
            (status, moved["state"].clone(), moved["checked"].clone()),
            "{name}: {moved}"
        );
        assert_eq!(
            400,
            call(
                &client,
                &marek,
                "PATCH",
                &format!("/tasks/tasks/{road}"),
                Some(json!({"state": "someday"}))
            )
            .0,
            "{name}"
        );

        // A card placed before another in its column.
        let buses = buses["id"].as_str().unwrap().to_string();
        let (_, info) = call(
            &client,
            &olena,
            "POST",
            "/tasks/tasks",
            Some(json!({"project": project, "title": "Update public info page"})),
        );
        let info = info["id"].as_str().unwrap().to_string();
        let (status, _) = call(
            &client,
            &olena,
            "PATCH",
            &format!("/tasks/tasks/{info}"),
            Some(json!({"state": "todo", "before": buses})),
        );
        assert_eq!(200, status, "{name}");
        let (_, board) = call(
            &client,
            &olena,
            "GET",
            &format!("/tasks/projects/{project}"),
            None,
        );
        let todo: Vec<&str> = board["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["state"] == "todo")
            .map(|t| t["title"].as_str().unwrap())
            .collect();
        assert_eq!(
            vec!["Update public info page", "Book 6 buses for evacuation"],
            todo,
            "{name}"
        );
        assert_eq!(
            json!("Marek Nowak").is_string(),
            board["people"][id_of("marek@ministry.example")]["name"].is_string(),
            "{name}: {board}"
        );

        // Marek's own list: the road task, with its project's name.
        let (_, mine) = call(&client, &marek, "GET", "/tasks/mine", None);
        assert_eq!(
            (
                json!("Close road R-12 and divert traffic"),
                json!("Flood response 2026")
            ),
            (
                mine["tasks"][0]["title"].clone(),
                mine["tasks"][0]["projectName"].clone()
            ),
            "{name}: {mine}"
        );
        let (_, projects) = call(&client, &marek, "GET", "/tasks/projects", None);
        assert_eq!(json!(1), projects["mine"], "{name}: {projects}");

        // Clearing the due time; renaming; members find it in search.
        let (_, cleared) = call(
            &client,
            &marek,
            "PATCH",
            &format!("/tasks/tasks/{road}"),
            Some(json!({"dueAt": null, "notes": "Detour via the ring road."})),
        );
        assert!(cleared["dueAt"].is_null(), "{name}: {cleared}");
        let find = |who: &str, q: &str| {
            ortus_core::runtime::block_on(ortus_core::search::search(
                &instance,
                &id_of(who),
                &ortus_core::search::Query::parse(q),
                10,
            ))
            .unwrap()
            .into_iter()
            .map(|h| h.title)
            .collect::<Vec<_>>()
        };
        assert_eq!(
            vec!["Close road R-12 and divert traffic".to_string()],
            find("marek@ministry.example", "ring road"),
            "{name}"
        );
        assert!(
            find("sam@ministry.example", "ring road").is_empty(),
            "{name}"
        );

        // Deleted: gone from the board and from search.
        assert_eq!(
            200,
            call(
                &client,
                &olena,
                "DELETE",
                &format!("/tasks/tasks/{info}"),
                None
            )
            .0,
            "{name}"
        );
        assert_eq!(
            404,
            call(
                &client,
                &olena,
                "GET",
                &format!("/tasks/tasks/{info}"),
                None
            )
            .0,
            "{name}"
        );

        // Only the owner takes people out; Marek leaves the company: his
        // tasks are nobody's, Olena's project stays hers.
        assert_eq!(
            403,
            call(
                &client,
                &marek,
                "PATCH",
                &format!("/tasks/projects/{project}"),
                Some(json!({"remove": [id_of("olena@ministry.example")]}))
            )
            .0,
            "{name}"
        );
        let marek_id = id_of("marek@ministry.example");
        ortus_core::runtime::block_on(async {
            let mut tx = instance.store.begin().await.unwrap();
            for app in instance.apps.iter() {
                app.forget_user(
                    tx.as_mut(),
                    &marek_id,
                    &ortus_core::retention::Rules::default(),
                )
                .await
                .unwrap();
            }
            tx.commit().await.unwrap();
        });
        let (_, after) = call(
            &client,
            &olena,
            "GET",
            &format!("/tasks/tasks/{road}"),
            None,
        );
        assert!(after["assignee"].is_null(), "{name}: {after}");
        // Sam's project with Olena in it passes to her when Sam leaves; its
        // tasks are still found.
        let (_, shared) = call(
            &client,
            &sam,
            "POST",
            "/tasks/projects",
            Some(json!({"name": "Depot", "members": ["olena@ministry.example"]})),
        );
        let shared = shared["id"].as_str().unwrap().to_string();
        call(
            &client,
            &sam,
            "POST",
            "/tasks/tasks",
            Some(json!({"project": shared, "title": "Count the sandbags in depot 2"})),
        );
        assert_eq!(
            1,
            find("olena@ministry.example", "sandbags").len(),
            "{name}"
        );
        // A project whose only member leaves goes with its tasks.
        let (_, solo) = call(
            &client,
            &sam,
            "POST",
            "/tasks/projects",
            Some(json!({"name": "Sam's list"})),
        );
        let solo = solo["id"].as_str().unwrap().to_string();
        call(
            &client,
            &sam,
            "POST",
            "/tasks/tasks",
            Some(json!({"project": solo, "title": "Water the plants"})),
        );
        let sam_id = id_of("sam@ministry.example");
        ortus_core::runtime::block_on(async {
            let mut tx = instance.store.begin().await.unwrap();
            for app in instance.apps.iter() {
                app.forget_user(
                    tx.as_mut(),
                    &sam_id,
                    &ortus_core::retention::Rules::default(),
                )
                .await
                .unwrap();
            }
            tx.commit().await.unwrap();
        });
        let left = ortus_core::runtime::block_on(instance.store.query(
            "SELECT COUNT(*) FROM tasks_items WHERE project_id = $1",
            &[solo.as_str().into()],
        ))
        .unwrap();
        assert_eq!(Some(0), left[0].int(0), "{name}");
        assert_eq!(
            1,
            find("olena@ministry.example", "sandbags").len(),
            "{name}: still found after Sam left"
        );
    }
}

/// The Tasks service other apps use (a call's action item): the task goes
/// into the creator's "Action items" project, the assignee is made a
/// member and hears of it; an assignee who is not a colleague is refused.
#[test]
fn other_apps_make_tasks_and_the_assignee_hears() {
    for (name, instance, _keep) in common::installations() {
        for app in instance.apps.clone() {
            app.provide(&instance, &instance.services);
        }
        let client = TestClient::new(ortus_core::app(Arc::clone(&instance)));
        let olena = signed_in(&instance, "olena@ministry.example", Role::Member);
        let marek = signed_in(&instance, "marek@ministry.example", Role::Member);
        let id = |e: &str| {
            ortus_core::runtime::block_on(ortus_core::auth::accounts::by_email(
                instance.store.as_ref(),
                e,
            ))
            .unwrap()
            .unwrap()
            .id
        };
        let (olena_id, marek_id) = (id("olena@ministry.example"), id("marek@ministry.example"));
        let tasks = ortus_core::runtime::block_on(
            instance
                .services
                .get::<dyn ortus_core::interfaces::Tasks>(&instance.switches),
        )
        .expect("tasks provides Tasks");
        let from = ortus_core::interfaces::ItemRef {
            app: "calls".into(),
            id: "c1".into(),
            path: "/calls?room=r1".into(),
        };
        let made = ortus_core::runtime::block_on(tasks.create(
            &olena_id,
            "Bus escort",
            &marek_id,
            Some(1_800_000_000_000),
            Some(from),
        ))
        .unwrap();
        assert!(made.path.starts_with("/tasks?task="), "{name}");
        // Again: the same project.
        ortus_core::runtime::block_on(tasks.create(
            &olena_id,
            "Open school shelter",
            &olena_id,
            None,
            None,
        ))
        .unwrap();
        let (_, mine) = call(&client, &marek, "GET", "/tasks/mine", None);
        assert!(mine.to_string().contains("Bus escort"), "{name}: {mine}");
        let (_, projects) = call(&client, &olena, "GET", "/tasks/projects", None);
        assert_eq!(
            1,
            projects["projects"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["name"] == "Action items")
                .count(),
            "{name}: {projects}"
        );
        let (_, his) = call(&client, &marek, "GET", "/notifications", None);
        let n = his["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["key"] == "task")
            .cloned()
            .expect("a task notification");
        assert_eq!(
            ("Bus escort", 1_800_000_000_000i64),
            (
                n["args"]["what"].as_str().unwrap(),
                n["args"]["due"].as_i64().unwrap()
            ),
            "{name}"
        );
        let (_, hers) = call(&client, &olena, "GET", "/notifications", None);
        assert!(
            !hers["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["key"] == "task"),
            "{name}: not for one's own"
        );
        assert!(
            ortus_core::runtime::block_on(tasks.create(
                &olena_id,
                "x",
                "nobody@nowhere.example",
                None,
                None
            ))
            .is_err(),
            "{name}"
        );
    }
}
