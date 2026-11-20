//! The tasks API: projects and their members, tasks on a project's board
//! (moved between columns and within one), one's own tasks, checklists.
//! Only a project's members see it; whoever is given a task becomes one.

use std::collections::HashMap;
use std::sync::Arc;

use ortus_core::api::{answer, body, ok, ApiError, Method, Route};
use ortus_core::auth::accounts::{self, User};
use ortus_core::auth::Access;
use ortus_core::instance::Instance;
use ortus_core::storage::BoxFuture;
use rust_web_server::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::index;
use crate::store::{self, now, value_opt_int, value_opt_text, Task, STATES};

pub(crate) async fn signed(request: &Request, instance: &Arc<Instance>) -> Result<User, ApiError> {
    Ok(ortus_core::auth::current(request, instance)
        .await
        .ok_or_else(ApiError::sign_in)?
        .user)
}

/// A path on this site (not `//other.site`, not `/\\other`).
pub(crate) fn local_path(p: &str) -> bool {
    p.starts_with('/') && !p.starts_with("//") && !p.starts_with("/\\") && !p.contains(['\r', '\n'])
}

pub(crate) fn clean(text: &str, max: usize) -> String {
    text.trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max)
        .collect()
}

/// A colleague by address (an unverified sign-up is not one).
pub(crate) async fn colleague(instance: &Instance, email: &str) -> Result<User, ApiError> {
    accounts::by_email(instance.store.as_ref(), &email.trim().to_ascii_lowercase())
        .await?
        .filter(|u| u.state != accounts::State::Unverified)
        .ok_or_else(|| ApiError::bad_request(format!("{} is not a colleague.", email.trim())))
}

/// Names and addresses of the people a screen shows, by id.
pub(crate) async fn people(instance: &Instance, ids: impl IntoIterator<Item = String>) -> Value {
    let mut out = serde_json::Map::new();
    for id in ids {
        if out.contains_key(&id) {
            continue;
        }
        if let Ok(Some(u)) = accounts::by_id(instance.store.as_ref(), &id).await {
            out.insert(id, json!({"name": u.name, "email": u.email}));
        }
    }
    Value::Object(out)
}

async fn member_project(
    instance: &Instance,
    user: &str,
    id: &str,
) -> Result<store::Project, ApiError> {
    store::project(instance.store.as_ref(), user, id)
        .await?
        .ok_or_else(|| ApiError::not_found("There is no such project."))
}

/// A task of a project the person is in.
async fn member_task(instance: &Instance, user: &str, id: &str) -> Result<Task, ApiError> {
    let s = instance.store.as_ref();
    let t = store::task(s, id)
        .await?
        .ok_or_else(|| ApiError::not_found("There is no such task."))?;
    if !store::is_member(s, &t.project, user).await? {
        return Err(ApiError::not_found("There is no such task."));
    }
    Ok(t)
}

fn valid_state(state: &str) -> Result<(), ApiError> {
    if STATES.contains(&state) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "The column is todo, doing, waiting or done.",
        ))
    }
}

// --- Projects ---

async fn list_projects(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let s = instance.store.as_ref();
    let mine = store::assigned_to(s, &me.id).await?;
    let open = mine.iter().filter(|t| t.state != "done").count();
    let list: Vec<Value> = store::projects_of(s, &me.id)
        .await?
        .iter()
        .map(|p| json!({"id": p.id, "name": p.name, "owner": p.owner}))
        .collect();
    Ok(ok(&json!({ "projects": list, "mine": open })))
}

#[derive(Deserialize)]
struct NewProject {
    name: String,
    #[serde(default)]
    members: Vec<String>,
}

async fn create_project(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let input: NewProject = body(request)?;
    let name = clean(&input.name, 200);
    if name.is_empty() {
        return Err(ApiError::bad_request("Give the project a name."));
    }
    if input.members.len() as i64 > store::MAX_MEMBERS {
        return Err(ApiError::bad_request("Too many members."));
    }
    let mut others = Vec::new();
    for email in &input.members {
        others.push(colleague(instance, email).await?.id);
    }
    let id = store::create_project(instance.store.as_ref(), &me.id, &name, &others).await?;
    Ok(ok(&json!({ "id": id, "name": name })))
}

async fn get_project(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let p = member_project(instance, &me.id, params.get("project").unwrap_or_default()).await?;
    let s = instance.store.as_ref();
    let tasks = store::tasks_in(s, &p.id).await?;
    let counts = store::check_counts(s, &p.id).await?;
    let members = store::members(s, &p.id).await?;
    let mut ids: Vec<String> = members.clone();
    ids.extend(tasks.iter().filter_map(|t| t.assignee.clone()));
    let list: Vec<Value> = tasks
        .iter()
        .map(|t| store::task_json(t, None, counts.get(&t.id).copied()))
        .collect();
    Ok(ok(&json!({
        "id": p.id, "name": p.name, "owner": p.owner,
        "members": members,
        "tasks": list,
        "people": people(instance, ids).await,
    })))
}

#[derive(Deserialize)]
struct ProjectChange {
    name: Option<String>,
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

async fn change_project(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let p = member_project(instance, &me.id, params.get("project").unwrap_or_default()).await?;
    let input: ProjectChange = body(request)?;
    let s = instance.store.as_ref();
    let mut adding = Vec::new();
    for email in &input.add {
        adding.push(colleague(instance, email).await?.id);
    }
    if store::members(s, &p.id).await?.len() + adding.len() > store::MAX_MEMBERS as usize {
        return Err(ApiError::bad_request("Too many members."));
    }
    // Only the owner takes people out; the owner stays.
    if !input.remove.is_empty() && p.owner != me.id {
        return Err(ApiError::forbidden());
    }
    let mut tx = s.begin().await?;
    if let Some(name) = input.name.as_deref().map(|n| clean(n, 200)) {
        if name.is_empty() {
            return Err(ApiError::bad_request("Give the project a name."));
        }
        tx.execute(
            "UPDATE tasks_projects SET name = $1 WHERE id = $2",
            &[name.as_str().into(), p.id.as_str().into()],
        )
        .await?;
    }
    for id in &adding {
        store::add_member(tx.as_mut(), &p.id, id).await?;
    }
    for id in input.remove.iter().filter(|id| **id != p.owner) {
        tx.execute(
            "DELETE FROM tasks_members WHERE project_id = $1 AND user_id = $2",
            &[p.id.as_str().into(), id.as_str().into()],
        )
        .await?;
        tx.execute(
            "UPDATE tasks_items SET assignee_id = NULL WHERE project_id = $1 AND assignee_id = $2",
            &[p.id.as_str().into(), id.as_str().into()],
        )
        .await?;
    }
    tx.commit().await?;
    get_project(request, params, instance).await
}

// --- Tasks ---

async fn mine(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let s = instance.store.as_ref();
    let tasks = store::assigned_to(s, &me.id).await?;
    let mut names: HashMap<String, String> = HashMap::new();
    for p in store::projects_of(s, &me.id).await? {
        names.insert(p.id, p.name);
    }
    let list: Vec<Value> = tasks
        .iter()
        .map(|t| {
            let mut v = store::task_json(t, None, None);
            v["projectName"] = json!(names.get(&t.project));
            v
        })
        .collect();
    Ok(ok(&json!({ "tasks": list })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewTask {
    project: String,
    title: String,
    #[serde(default)]
    notes: String,
    /// The address of who does it.
    assignee: Option<String>,
    due_at: Option<i64>,
    state: Option<String>,
    #[serde(default)]
    checklist: Vec<String>,
    /// Where it came from (a chat message's address, a document).
    source: Option<String>,
}

/// Makes a task (the API and the Tasks service): its row, its checklist,
/// the assignee made a member; the assignee hears of it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_task(
    instance: &Instance,
    me: &ortus_core::auth::accounts::User,
    project: &str,
    title: &str,
    notes: &str,
    assignee: Option<&str>,
    due_at: Option<i64>,
    state: &str,
    source: Option<&str>,
    checklist: &[String],
) -> Result<String, ApiError> {
    let s = instance.store.as_ref();
    let id = store::new_id("t");
    let at = now();
    let mut tx = s.begin().await?;
    let position = store::next_position(tx.as_mut(), project, state).await?;
    tx.execute(
        &format!("INSERT INTO tasks_items ({}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11, $12)", store::TASK_COLUMNS),
        &[
            id.as_str().into(),
            project.into(),
            title.into(),
            notes.into(),
            value_opt_text(assignee),
            value_opt_int(due_at),
            state.into(),
            position.into(),
            me.id.as_str().into(),
            at.into(),
            value_opt_int((state == "done").then_some(at)),
            value_opt_text(source),
        ],
    )
    .await?;
    if let Some(a) = assignee {
        store::add_member(tx.as_mut(), project, a).await?;
    }
    for (n, text) in checklist.iter().enumerate() {
        let text = clean(text, 300);
        if text.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO tasks_checks (task_id, n, text, done) VALUES ($1, $2, $3, 0)",
            &[id.as_str().into(), (n as i64).into(), text.as_str().into()],
        )
        .await?;
    }
    // Someone else's task: they hear of it.
    if let Some(a) = assignee.filter(|a| *a != me.id) {
        let mut args = serde_json::json!({"who": me.name, "what": title});
        if let Some(d) = due_at {
            args["due"] = serde_json::json!(d);
        }
        let notice = ortus_core::notify::Notice {
            app: "tasks",
            kind: ortus_core::notify::Kind::Update,
            title: &format!("{} gave you a task: {title}", me.name),
            detail: "",
            path: &format!("/tasks?task={id}"),
            key: Some("task"),
            args,
        };
        ortus_core::notify::add_in(tx.as_mut(), a, &notice).await?;
    }
    tx.commit().await?;
    ortus_core::auth::audit(
        instance,
        ortus_core::audit::Event::new("tasks.created", ortus_core::audit::Outcome::Done)
            .by(&me.id)
            .on(&id),
    )
    .await;
    index::task(instance, &id).await;
    Ok(id)
}

async fn create_task(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let input: NewTask = body(request)?;
    let p = member_project(instance, &me.id, &input.project).await?;
    let title = clean(&input.title, 300);
    if title.is_empty() {
        return Err(ApiError::bad_request("Give the task a title."));
    }
    let state = input.state.unwrap_or_else(|| "todo".into());
    valid_state(&state)?;
    if input.checklist.len() as i64 > store::MAX_CHECKS {
        return Err(ApiError::bad_request("Too many checklist items."));
    }
    let s = instance.store.as_ref();
    if store::count_in(s, &p.id).await? >= store::MAX_TASKS {
        return Err(ApiError::bad_request(
            "The project has as many tasks as it can hold.",
        ));
    }
    let assignee = match input.assignee.as_deref().filter(|a| !a.trim().is_empty()) {
        Some(email) => Some(colleague(instance, email).await?.id),
        None => None,
    };
    let source = input
        .source
        .as_deref()
        .map(|x| clean(x, 500))
        .filter(|x| crate::api::local_path(x));
    let id = insert_task(
        instance,
        &me,
        &p.id,
        &title,
        &clean(&input.notes, 20_000),
        assignee.as_deref(),
        input.due_at,
        &state,
        source.as_deref(),
        &input.checklist,
    )
    .await?;
    get_task_of(instance, &id).await
}

async fn get_task_of(instance: &Instance, id: &str) -> Result<Response, ApiError> {
    let s = instance.store.as_ref();
    let t = store::task(s, id)
        .await?
        .ok_or_else(|| ApiError::not_found("There is no such task."))?;
    let checks = store::checks(s, id).await?;
    let mut v = store::task_json(&t, Some(&checks), None);
    v["people"] = people(
        instance,
        t.assignee.iter().cloned().chain([t.created_by.clone()]),
    )
    .await;
    Ok(ok(&v))
}

async fn get_task(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let t = member_task(instance, &me.id, params.get("task").unwrap_or_default()).await?;
    get_task_of(instance, &t.id).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskChange {
    title: Option<String>,
    notes: Option<String>,
    /// An address, or "" for nobody.
    assignee: Option<String>,
    /// A time, or null to clear it (absent: unchanged).
    #[serde(default, deserialize_with = "some_or_null")]
    due_at: Option<Option<i64>>,
    state: Option<String>,
    /// Placed before this task in the column (absent: at the end).
    before: Option<String>,
}

fn some_or_null<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Ok(Some(Option::<i64>::deserialize(d)?))
}

async fn change_task(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let t = member_task(instance, &me.id, params.get("task").unwrap_or_default()).await?;
    let input: TaskChange = body(request)?;
    let s = instance.store.as_ref();
    let assignee = match input.assignee.as_deref().map(str::trim) {
        Some("") => Some(None),
        Some(email) => Some(Some(colleague(instance, email).await?.id)),
        None => None,
    };
    if let Some(st) = &input.state {
        valid_state(st)?;
    }
    let mut tx = s.begin().await?;
    let mut set: Vec<(&str, ortus_core::storage::Value)> = Vec::new();
    if let Some(title) = input.title.as_deref().map(|x| clean(x, 300)) {
        if title.is_empty() {
            return Err(ApiError::bad_request("Give the task a title."));
        }
        set.push(("title", title.into()));
    }
    if let Some(notes) = &input.notes {
        set.push(("notes", clean(notes, 20_000).into()));
    }
    if let Some(a) = &assignee {
        set.push(("assignee_id", value_opt_text(a.as_deref())));
        if let Some(a) = a {
            store::add_member(tx.as_mut(), &t.project, a).await?;
        }
    }
    if let Some(due) = input.due_at {
        set.push(("due_at", value_opt_int(due)));
    }
    let state = input.state.clone().unwrap_or_else(|| t.state.clone());
    if input.state.is_some() || input.before.is_some() {
        // A place in the column: just before the named card, or the end.
        let place = match input.before.as_deref() {
            Some(other) => {
                let o = tx
                    .query(
                        "SELECT position, state FROM tasks_items WHERE id = $1 AND project_id = $2",
                        &[other.into(), t.project.as_str().into()],
                    )
                    .await?;
                let o = o
                    .first()
                    .filter(|r| r.text(1) == Some(state.as_str()))
                    .and_then(|r| r.int(0))
                    .ok_or_else(|| ApiError::bad_request("That card is not in the column."))?;
                let prev = tx
                    .query(
                        "SELECT CAST(COALESCE(MAX(position), 0) AS BIGINT) FROM tasks_items WHERE project_id = $1 AND state = $2 AND position < $3 AND id <> $4",
                        &[t.project.as_str().into(), state.as_str().into(), o.into(), t.id.as_str().into()],
                    )
                    .await?
                    .first()
                    .and_then(|r| r.int(0))
                    .unwrap_or(0);
                if o - prev < 2 {
                    // No room between: spread the column out again.
                    let ids: Vec<String> = tx
                        .query(
                            "SELECT id FROM tasks_items WHERE project_id = $1 AND state = $2 ORDER BY position, created_at",
                            &[t.project.as_str().into(), state.as_str().into()],
                        )
                        .await?
                        .iter()
                        .filter_map(|r| r.text(0).map(str::to_string))
                        .collect();
                    let mut at = 0;
                    let mut found = 0;
                    for id in ids.iter().filter(|id| **id != t.id) {
                        at += 1024;
                        if *id == other {
                            found = at;
                            at += 1024;
                        }
                        tx.execute(
                            "UPDATE tasks_items SET position = $1 WHERE id = $2",
                            &[at.into(), id.as_str().into()],
                        )
                        .await?;
                    }
                    found
                } else {
                    (o + prev) / 2
                }
            }
            None => store::next_position(tx.as_mut(), &t.project, &state).await?,
        };
        set.push(("position", place.into()));
        set.push(("state", state.as_str().into()));
        if state == "done" && t.state != "done" {
            set.push(("done_at", now().into()));
        } else if state != "done" {
            set.push(("done_at", ortus_core::storage::Value::Null));
        }
    }
    set.push(("updated_at", now().into()));
    let assignments: Vec<String> = set
        .iter()
        .enumerate()
        .map(|(i, (col, _))| format!("{col} = ${}", i + 1))
        .collect();
    let mut values: Vec<ortus_core::storage::Value> = set.into_iter().map(|(_, v)| v).collect();
    values.push(t.id.as_str().into());
    tx.execute(
        &format!(
            "UPDATE tasks_items SET {} WHERE id = ${}",
            assignments.join(", "),
            values.len()
        ),
        &values,
    )
    .await?;
    tx.commit().await?;
    index::task(instance, &t.id).await;
    get_task_of(instance, &t.id).await
}

async fn delete_task(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let t = member_task(instance, &me.id, params.get("task").unwrap_or_default()).await?;
    let s = instance.store.as_ref();
    let mut tx = s.begin().await?;
    tx.execute(
        "DELETE FROM tasks_checks WHERE task_id = $1",
        &[t.id.as_str().into()],
    )
    .await?;
    tx.execute(
        "DELETE FROM tasks_items WHERE id = $1",
        &[t.id.as_str().into()],
    )
    .await?;
    ortus_core::search::remove_in(tx.as_mut(), s.engine(), "tasks", &t.id).await?;
    tx.commit().await?;
    ortus_core::auth::audit(
        instance,
        ortus_core::audit::Event::new("tasks.deleted", ortus_core::audit::Outcome::Done)
            .by(&me.id)
            .on(&t.id),
    )
    .await;
    Ok(ok(&json!({ "deleted": t.id })))
}

#[derive(Deserialize)]
struct NewCheck {
    text: String,
}

async fn add_check(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let t = member_task(instance, &me.id, params.get("task").unwrap_or_default()).await?;
    let input: NewCheck = body(request)?;
    let text = clean(&input.text, 300);
    if text.is_empty() {
        return Err(ApiError::bad_request("Write the item."));
    }
    let s = instance.store.as_ref();
    let mut tx = s.begin().await?;
    let rows = tx
        .query(
            "SELECT CAST(COUNT(*) AS BIGINT), CAST(COALESCE(MAX(n), -1) AS BIGINT) FROM tasks_checks WHERE task_id = $1",
            &[t.id.as_str().into()],
        )
        .await?;
    let (count, last) = rows
        .first()
        .map(|r| (r.int(0).unwrap_or(0), r.int(1).unwrap_or(-1)))
        .unwrap_or((0, -1));
    if count >= store::MAX_CHECKS {
        return Err(ApiError::bad_request("Too many checklist items."));
    }
    tx.execute(
        "INSERT INTO tasks_checks (task_id, n, text, done) VALUES ($1, $2, $3, 0)",
        &[
            t.id.as_str().into(),
            (last + 1).into(),
            text.as_str().into(),
        ],
    )
    .await?;
    tx.commit().await?;
    get_task_of(instance, &t.id).await
}

#[derive(Deserialize)]
struct CheckChange {
    done: Option<bool>,
    text: Option<String>,
}

async fn change_check(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let t = member_task(instance, &me.id, params.get("task").unwrap_or_default()).await?;
    let n: i64 = params
        .get("n")
        .unwrap_or_default()
        .parse()
        .map_err(|_| ApiError::not_found("There is no such item."))?;
    let input: CheckChange = body(request)?;
    let s = instance.store.as_ref();
    if let Some(done) = input.done {
        s.execute(
            "UPDATE tasks_checks SET done = $1 WHERE task_id = $2 AND n = $3",
            &[i64::from(done).into(), t.id.as_str().into(), n.into()],
        )
        .await?;
    }
    if let Some(text) = input.text.as_deref().map(|x| clean(x, 300)) {
        if text.is_empty() {
            s.execute(
                "DELETE FROM tasks_checks WHERE task_id = $1 AND n = $2",
                &[t.id.as_str().into(), n.into()],
            )
            .await?;
        } else {
            s.execute(
                "UPDATE tasks_checks SET text = $1 WHERE task_id = $2 AND n = $3",
                &[text.as_str().into(), t.id.as_str().into(), n.into()],
            )
            .await?;
        }
    }
    get_task_of(instance, &t.id).await
}

macro_rules! plain {
    ($name:ident, $f:ident) => {
        fn $name(
            request: Request,
            _: PathParams,
            _: ConnectionInfo,
            instance: Arc<Instance>,
        ) -> BoxFuture<'static, Response> {
            Box::pin(async move { answer(&request, $f(&request, &instance).await) })
        }
    };
}
macro_rules! with_params {
    ($name:ident, $f:ident) => {
        fn $name(
            request: Request,
            params: PathParams,
            _: ConnectionInfo,
            instance: Arc<Instance>,
        ) -> BoxFuture<'static, Response> {
            Box::pin(async move { answer(&request, $f(&request, &params, &instance).await) })
        }
    };
}

plain!(h_projects, list_projects);
plain!(h_new_project, create_project);
with_params!(h_project, get_project);
with_params!(h_change_project, change_project);
plain!(h_mine, mine);
plain!(h_new_task, create_task);
with_params!(h_task, get_task);
with_params!(h_change_task, change_task);
with_params!(h_delete_task, delete_task);
with_params!(h_add_check, add_check);
with_params!(h_change_check, change_check);

pub(crate) fn object() -> Value {
    json!({"type": "object"})
}

pub fn routes() -> Vec<Route> {
    let route = |method, path, id, summary, handler| Route {
        method,
        path,
        id,
        tag: "tasks",
        summary,
        access: Access::Signed,
        response: object,
        handler,
    };
    vec![
        route(Method::Get, "/tasks/projects", "tasksProjects", "The projects one is a member of, and how many open tasks one has.", h_projects as ortus_core::api::Handler),
        route(Method::Post, "/tasks/projects", "tasksProjectCreate", "A new project: {name, members: [address]}.", h_new_project),
        route(Method::Get, "/tasks/projects/:project", "tasksProject", "A project: its members and its tasks with their checklist counts.", h_project),
        route(Method::Patch, "/tasks/projects/:project", "tasksProjectChange", "Renames a project, adds members by address; its owner takes members out.", h_change_project),
        route(Method::Get, "/tasks/mine", "tasksMine", "One's tasks across projects: open ones by due time, then those done in the last two weeks.", h_mine),
        route(Method::Post, "/tasks/tasks", "tasksCreate", "A new task: {project, title, notes, assignee (address), dueAt, state, checklist, source}.", h_new_task),
        route(Method::Get, "/tasks/tasks/:task", "tasksGet", "A task with its checklist.", h_task),
        route(Method::Patch, "/tasks/tasks/:task", "tasksChange", "Changes a task; {state, before} moves it on the board.", h_change_task),
        route(Method::Delete, "/tasks/tasks/:task", "tasksDelete", "Deletes a task.", h_delete_task),
        route(Method::Post, "/tasks/tasks/:task/checks", "tasksCheckAdd", "Adds a checklist item: {text}.", h_add_check),
        route(Method::Patch, "/tasks/tasks/:task/checks/:n", "tasksCheckChange", "Ticks or renames a checklist item; empty text removes it.", h_change_check),
    ]
}
