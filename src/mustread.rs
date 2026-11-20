//! Read and confirm: policies an administrator publishes as must-read
//! to everyone or to groups (their text kept as published, from a
//! document or a record); each person confirms they read each one, and
//! the publisher sees who did.

use std::sync::Arc;

use ortus_core::api::{answer, body, ok, ApiError, Method, Route};
use ortus_core::auth::accounts::Role;
use ortus_core::auth::Access;
use ortus_core::instance::Instance;
use ortus_core::storage::{AsyncTx, BoxFuture, Migration, StoreResult};
use rust_web_server::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{clean, people, signed};
use crate::store::{new_id, now};

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 2,
    name: "must-read policies and confirmations",
    sql: "CREATE TABLE records_mustread (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        note TEXT NOT NULL,
        body TEXT NOT NULL,
        minutes BIGINT NOT NULL,
        record_id TEXT,
        source TEXT NOT NULL,
        audience TEXT NOT NULL,
        published_by TEXT NOT NULL,
        published_at BIGINT NOT NULL,
        withdrawn_at BIGINT
    );
    CREATE TABLE records_confirmations (
        item_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        at BIGINT NOT NULL,
        PRIMARY KEY (item_id, user_id)
    )",
}];

/// A published body is at most this large (ProseMirror JSON).
const MAX_BODY: usize = 512 * 1024;

fn words(v: &Value) -> usize {
    match v {
        Value::String(s) => s.split_whitespace().count(),
        Value::Array(a) => a.iter().map(words).sum(),
        Value::Object(o) => o
            .iter()
            .filter(|(k, _)| *k == "text" || *k == "content")
            .map(|(_, v)| words(v))
            .sum(),
        _ => 0,
    }
}

async fn groups_of(instance: &Instance, user: &str) -> StoreResult<Vec<String>> {
    Ok(instance
        .store
        .query(
            "SELECT g.name FROM core_groups g JOIN core_group_members m ON m.group_id = g.id WHERE m.user_id = $1",
            &[user.into()],
        )
        .await?
        .iter()
        .filter_map(|r| r.text(0).map(str::to_string))
        .collect())
}

fn for_me(audience: &str, groups: &[String]) -> bool {
    audience == "*" || audience.split('\n').any(|g| groups.iter().any(|m| m == g))
}

#[derive(Deserialize)]
struct Publish {
    title: String,
    #[serde(default)]
    note: String,
    /// The text, as the editor's structure (ProseMirror JSON).
    body: Value,
    /// Where it came from (a document, a record).
    #[serde(default)]
    source: String,
    record: Option<String>,
    /// Group names; empty or absent: everyone.
    #[serde(default)]
    groups: Vec<String>,
}

async fn publish(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    if me.role < Role::Admin {
        return Err(ApiError::forbidden());
    }
    let input: Publish = body(request)?;
    let title = clean(&input.title, 200);
    let text = input.body.to_string();
    if title.is_empty() || text.len() > MAX_BODY || input.body["type"] != "doc" {
        return Err(ApiError::bad_request(
            "A must-read has a title and a document's text.",
        ));
    }
    let audience = if input.groups.is_empty() {
        "*".to_string()
    } else {
        input
            .groups
            .iter()
            .map(|g| clean(g, 100))
            .filter(|g| !g.is_empty() && g != "*")
            .collect::<Vec<_>>()
            .join("\n")
    };
    let source = Some(clean(&input.source, 300))
        .filter(|x| crate::api::local_path(x))
        .unwrap_or_default();
    let id = new_id("m");
    let minutes = (words(&input.body) as i64 / 200).max(1);
    instance
        .store
        .execute(
            "INSERT INTO records_mustread (id, title, note, body, minutes, record_id, source, audience, published_by, published_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            &[id.as_str().into(), title.as_str().into(), clean(&input.note, 300).into(), text.into(), minutes.into(), crate::store::value_opt_text(input.record.as_deref()), source.into(), audience.into(), me.id.as_str().into(), now().into()],
        )
        .await?;
    if let Some(record) = &input.record {
        crate::records::note(instance.store.as_ref(), record, "must-read", &title).await?;
    }
    ortus_core::auth::audit(
        instance,
        ortus_core::audit::Event::new(
            "records.must_read_published",
            ortus_core::audit::Outcome::Done,
        )
        .by(&me.id)
        .on(&id)
        .with(json!({"title": title})),
    )
    .await;
    Ok(ok(&json!({ "id": id })))
}

async fn mine(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let groups = groups_of(instance, &me.id).await?;
    let rows = instance
        .store
        .query(
            "SELECT m.id, m.title, m.note, m.body, m.minutes, m.source, m.audience, m.published_at, c.at, m.published_by
             FROM records_mustread m LEFT JOIN records_confirmations c ON c.item_id = m.id AND c.user_id = $1
             WHERE m.withdrawn_at IS NULL ORDER BY m.published_at",
            &[me.id.as_str().into()],
        )
        .await?;
    let mut list = Vec::new();
    let mut by = Vec::new();
    for r in rows
        .iter()
        .filter(|r| for_me(r.text(6).unwrap_or_default(), &groups))
    {
        by.push(r.text(9).unwrap_or_default().to_string());
        list.push(json!({
            "id": r.text(0), "title": r.text(1), "note": r.text(2),
            "body": serde_json::from_str::<Value>(r.text(3).unwrap_or("null")).unwrap_or(Value::Null),
            "minutes": r.int(4), "source": r.text(5), "publishedAt": r.int(7), "confirmedAt": r.int(8), "by": r.text(9),
        }));
    }
    Ok(ok(
        &json!({ "items": list, "people": people(instance, by).await }),
    ))
}

async fn confirm(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let id = params.get("item").unwrap_or_default();
    let groups = groups_of(instance, &me.id).await?;
    let s = instance.store.as_ref();
    let found = s
        .query(
            "SELECT audience FROM records_mustread WHERE id = $1 AND withdrawn_at IS NULL",
            &[id.into()],
        )
        .await?;
    match found.first() {
        Some(r) if for_me(r.text(0).unwrap_or_default(), &groups) => {}
        _ => return Err(ApiError::not_found("There is no such policy.")),
    }
    let mut tx = s.begin().await?;
    let there = !tx
        .query(
            "SELECT 1 FROM records_confirmations WHERE item_id = $1 AND user_id = $2",
            &[id.into(), me.id.as_str().into()],
        )
        .await?
        .is_empty();
    if !there {
        tx.execute(
            "INSERT INTO records_confirmations (item_id, user_id, at) VALUES ($1, $2, $3)",
            &[id.into(), me.id.as_str().into(), now().into()],
        )
        .await?;
    }
    tx.commit().await?;
    ortus_core::auth::audit(
        instance,
        ortus_core::audit::Event::new(
            "records.must_read_confirmed",
            ortus_core::audit::Outcome::Done,
        )
        .by(&me.id)
        .on(id),
    )
    .await;
    mine(request, instance).await
}

/// Who confirmed a must-read: its publisher and administrators.
async fn confirmations(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let id = params.get("item").unwrap_or_default();
    let s = instance.store.as_ref();
    let rows = s
        .query(
            "SELECT published_by FROM records_mustread WHERE id = $1",
            &[id.into()],
        )
        .await?;
    let by = rows
        .first()
        .and_then(|r| r.text(0).map(str::to_string))
        .ok_or_else(|| ApiError::not_found("There is no such policy."))?;
    if by != me.id && me.role < Role::Admin {
        return Err(ApiError::not_found("There is no such policy."));
    }
    let list: Vec<(String, i64)> = s
        .query(
            "SELECT user_id, at FROM records_confirmations WHERE item_id = $1 ORDER BY at",
            &[id.into()],
        )
        .await?
        .iter()
        .map(|r| {
            (
                r.text(0).unwrap_or_default().to_string(),
                r.int(1).unwrap_or(0),
            )
        })
        .collect();
    let ids: Vec<String> = list.iter().map(|(u, _)| u.clone()).collect();
    let names = people(instance, ids).await;
    Ok(ok(&json!({
        "confirmed": list.iter().map(|(u, at)| json!({"user": u, "at": at, "name": names[u]["name"], "email": names[u]["email"]})).collect::<Vec<_>>(),
    })))
}

fn h_publish(
    request: Request,
    _: PathParams,
    _: ConnectionInfo,
    instance: Arc<Instance>,
) -> BoxFuture<'static, Response> {
    Box::pin(async move { answer(&request, publish(&request, &instance).await) })
}
fn h_mine(
    request: Request,
    _: PathParams,
    _: ConnectionInfo,
    instance: Arc<Instance>,
) -> BoxFuture<'static, Response> {
    Box::pin(async move { answer(&request, mine(&request, &instance).await) })
}
fn h_confirm(
    request: Request,
    params: PathParams,
    _: ConnectionInfo,
    instance: Arc<Instance>,
) -> BoxFuture<'static, Response> {
    Box::pin(async move { answer(&request, confirm(&request, &params, &instance).await) })
}
fn h_confirmations(
    request: Request,
    params: PathParams,
    _: ConnectionInfo,
    instance: Arc<Instance>,
) -> BoxFuture<'static, Response> {
    Box::pin(async move { answer(&request, confirmations(&request, &params, &instance).await) })
}

pub fn routes() -> Vec<Route> {
    let route = |method, path, id, summary, handler| Route {
        method,
        path,
        id,
        tag: "records",
        summary,
        access: Access::Signed,
        response: crate::api::object,
        handler,
    };
    vec![
        route(Method::Post, "/records/must-read", "recordsMustReadPublish", "An administrator publishes a must-read: {title, note, body (a document's structure), source, record, groups (none: everyone)}.", h_publish as ortus_core::api::Handler),
        route(Method::Get, "/records/must-read", "recordsMustReadMine", "The must-reads one is asked to read, oldest first, with when one confirmed each.", h_mine),
        route(Method::Post, "/records/must-read/:item/confirm", "recordsMustReadConfirm", "One confirms one read and understood a must-read.", h_confirm),
        route(Method::Get, "/records/must-read/:item/confirmations", "recordsMustReadConfirmations", "Who confirmed a must-read, for its publisher and administrators.", h_confirmations),
    ]
}

/// A person leaves: their confirmations go with them.
pub async fn forget(tx: &mut dyn AsyncTx, user: &str) -> StoreResult<()> {
    tx.execute(
        "DELETE FROM records_confirmations WHERE user_id = $1",
        &[user.into()],
    )
    .await?;
    tx.execute(
        "UPDATE records_mustread SET published_by = '' WHERE published_by = $1",
        &[user.into()],
    )
    .await
    .map(|_| ())
}
