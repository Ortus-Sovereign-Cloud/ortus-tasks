//! Tasks in search: a task's title and notes, found by the members of
//! its project (the entry is the project owner's; the app vouches for
//! the other members).

use ortus_core::instance::Instance;
use serde_json::json;

use crate::store;

/// Indexes a task again (after it was made or changed).
pub async fn task(instance: &Instance, id: &str) {
    let s = instance.store.as_ref();
    let Ok(Some(t)) = store::task(s, id).await else {
        return;
    };
    let Ok(Some(owner)) = s
        .query(
            "SELECT owner_id FROM tasks_projects WHERE id = $1",
            &[t.project.as_str().into()],
        )
        .await
        .map(|r| r.first().and_then(|r| r.text(0).map(str::to_string)))
    else {
        return;
    };
    let entry = ortus_core::search::Entry {
        app: "tasks",
        id: &t.id,
        owner: &owner,
        title: &t.title,
        body: &t.notes,
        meta: json!({"path": format!("/tasks?task={}", t.id), "kind": "task", "state": t.state, "due": t.due_at}),
        at: t.updated_at,
    };
    let _ = ortus_core::search::index(s, &entry).await;
}

/// Owners of projects the person is in.
pub async fn owners_for(instance: &Instance, user: &str) -> Vec<String> {
    store::projects_of(instance.store.as_ref(), user)
        .await
        .map(|ps| {
            ps.into_iter()
                .map(|p| p.owner)
                .filter(|o| o != user)
                .collect()
        })
        .unwrap_or_default()
}

/// Of these tasks, those in the person's projects.
pub async fn visible(instance: &Instance, user: &str, ids: Vec<String>) -> Vec<String> {
    let s = instance.store.as_ref();
    let mut out = Vec::new();
    for id in ids {
        if let Ok(Some(t)) = store::task(s, &id).await {
            if store::is_member(s, &t.project, user).await.unwrap_or(false) {
                out.push(id);
            }
        }
    }
    out
}
