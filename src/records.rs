//! Records: final PDFs filed to a case, never changed or deleted while
//! they are kept (ten years for decisions). Filed by an approval route at
//! its end; read by the route's people and by administrators; their
//! signatures can be checked at any time.

use std::sync::{Arc, Weak};

use ortus_core::api::{answer, ok, ApiError, Method, Route};
use ortus_core::auth::accounts::Role;
use ortus_core::auth::Access;
use ortus_core::instance::Instance;
use ortus_core::interfaces::{ItemRef, NewRecord};
use ortus_core::storage::{AsyncStore, AsyncTx, BoxFuture, Migration, StoreResult};
use rust_web_server::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::api::{people, signed};
use crate::store::{new_id, now};

pub const MIGRATIONS: &[Migration] = &[RECORDS, crate::mustread::MIGRATIONS[0]];

const RECORDS: Migration = Migration {
    version: 1,
    name: "cases, records, readers, what happened",
    sql: "CREATE TABLE records_cases (
        id TEXT PRIMARY KEY,
        reference TEXT NOT NULL UNIQUE,
        created_at BIGINT NOT NULL
    );
    CREATE TABLE records_items (
        id TEXT PRIMARY KEY,
        case_id TEXT NOT NULL,
        reference TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL,
        kind TEXT NOT NULL,
        label TEXT,
        file_key TEXT NOT NULL,
        sha256 TEXT NOT NULL,
        size BIGINT NOT NULL,
        filed_by TEXT NOT NULL,
        filed_at BIGINT NOT NULL,
        source TEXT NOT NULL,
        keep_until BIGINT NOT NULL
    );
    CREATE INDEX records_items_case ON records_items (case_id, filed_at);
    CREATE TABLE records_readers (
        record_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        PRIMARY KEY (record_id, user_id)
    );
    CREATE INDEX records_readers_user ON records_readers (user_id);
    CREATE TABLE records_events (
        record_id TEXT NOT NULL,
        at BIGINT NOT NULL,
        kind TEXT NOT NULL,
        detail TEXT NOT NULL
    );
    CREATE INDEX records_events_record ON records_events (record_id, at)",
};

/// How long a record of a kind is kept, in years.
pub fn keep_years(kind: &str) -> i64 {
    match kind {
        "contract" => 7,
        _ => 10,
    }
}

fn year_of(ms: i64) -> i64 {
    // Days since 1970 to the civil year (proleptic Gregorian).
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    yoe + era * 400 + i64::from(mp >= 10)
}

/// A case reference as people write it: letters, digits, / and -.
fn clean_case(reference: &str) -> Option<String> {
    let r: String = reference
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-'))
        .take(40)
        .collect::<String>()
        .to_ascii_uppercase();
    (!r.is_empty()).then_some(r)
}

async fn case_id(tx: &mut dyn AsyncTx, reference: &str) -> StoreResult<String> {
    if let Some(id) = tx
        .query(
            "SELECT id FROM records_cases WHERE reference = $1",
            &[reference.into()],
        )
        .await?
        .first()
        .and_then(|r| r.text(0).map(str::to_string))
    {
        return Ok(id);
    }
    let id = new_id("k");
    tx.execute(
        "INSERT INTO records_cases (id, reference, created_at) VALUES ($1, $2, $3)",
        &[id.as_str().into(), reference.into(), now().into()],
    )
    .await?;
    Ok(id)
}

/// Files a record: the PDF goes to the file store first, then its row.
pub async fn file(instance: &Instance, r: NewRecord<'_>) -> Result<ItemRef, String> {
    let at = now();
    let case = r
        .case
        .and_then(clean_case)
        .unwrap_or_else(|| format!("{}/GEN", year_of(at)));
    let id = new_id("r");
    let key = format!("records/{id}.pdf");
    instance
        .files
        .put(&key, r.pdf)
        .await
        .map_err(|e| e.to_string())?;
    let digest: String = Sha256::digest(r.pdf)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let s = instance.store.as_ref();
    let mut tx = s.begin().await.map_err(|e| e.to_string())?;
    let case_id = case_id(tx.as_mut(), &case)
        .await
        .map_err(|e| e.to_string())?;
    let n = tx
        .query(
            "SELECT COUNT(*) FROM records_items WHERE case_id = $1",
            &[case_id.as_str().into()],
        )
        .await
        .map_err(|e| e.to_string())?
        .first()
        .and_then(|row| row.int(0))
        .unwrap_or(0)
        + 1;
    let short = case.rsplit('/').next().unwrap_or(&case).to_string();
    let reference = format!("{short}-{n:03}");
    let keep_until = at + keep_years(r.kind) * 365 * 86_400_000 + 3 * 86_400_000;
    tx.execute(
        "INSERT INTO records_items (id, case_id, reference, title, kind, label, file_key, sha256, size, filed_by, filed_at, source, keep_until) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        &[
            id.as_str().into(),
            case_id.as_str().into(),
            reference.as_str().into(),
            r.title.into(),
            r.kind.into(),
            crate::store::value_opt_text(r.label),
            key.as_str().into(),
            digest.as_str().into(),
            (r.pdf.len() as i64).into(),
            r.by.into(),
            at.into(),
            r.source.into(),
            keep_until.into(),
        ],
    )
    .await
    .map_err(|e| e.to_string())?;
    let mut readers: Vec<&String> = r.readers.iter().collect();
    readers.sort();
    readers.dedup();
    for u in readers {
        tx.execute(
            "INSERT INTO records_readers (record_id, user_id) VALUES ($1, $2)",
            &[id.as_str().into(), u.as_str().into()],
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.execute(
        "INSERT INTO records_events (record_id, at, kind, detail) VALUES ($1, $2, 'filed', $3)",
        &[id.as_str().into(), at.into(), case.as_str().into()],
    )
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    ortus_core::auth::audit(
        instance,
        ortus_core::audit::Event::new("records.filed", ortus_core::audit::Outcome::Done)
            .by(r.by)
            .on(&id)
            .with(json!({"reference": reference, "case": case, "sha256": digest})),
    )
    .await;
    let entry = ortus_core::search::Entry {
        app: "records",
        id: &id,
        owner: r.by,
        title: r.title,
        body: &format!("{reference} {case}"),
        meta: json!({"path": format!("/records?record={id}"), "kind": "record"}),
        at,
    };
    let _ = ortus_core::search::index(s, &entry).await;
    Ok(ItemRef {
        app: "records".into(),
        id: id.clone(),
        path: format!("/records?record={id}"),
    })
}

/// Something that happened to a record afterwards ("published as must-read").
pub async fn note(
    store: &dyn AsyncStore,
    record: &str,
    kind: &str,
    detail: &str,
) -> StoreResult<()> {
    store
        .execute(
            "INSERT INTO records_events (record_id, at, kind, detail) VALUES ($1, $2, $3, $4)",
            &[record.into(), now().into(), kind.into(), detail.into()],
        )
        .await
        .map(|_| ())
}

pub struct Service {
    pub instance: Weak<Instance>,
}

impl ortus_core::interfaces::Records for Service {
    fn file<'a>(&'a self, record: NewRecord<'a>) -> BoxFuture<'a, Result<ItemRef, String>> {
        Box::pin(async move {
            let instance = self.instance.upgrade().ok_or("stopping")?;
            file(&instance, record).await
        })
    }
}

/// Whether someone may read a record: one of its readers, who filed it,
/// or an administrator.
pub async fn may_read(
    instance: &Instance,
    user: &ortus_core::auth::accounts::User,
    record: &str,
) -> StoreResult<bool> {
    if user.role >= Role::Admin {
        return Ok(true);
    }
    Ok(!instance
        .store
        .query(
            "SELECT 1 FROM records_items i WHERE i.id = $1 AND (i.filed_by = $2 OR EXISTS (SELECT 1 FROM records_readers r WHERE r.record_id = i.id AND r.user_id = $2))",
            &[record.into(), user.id.as_str().into()],
        )
        .await?
        .is_empty())
}

const COLUMNS: &str = "i.id, i.reference, i.title, i.kind, i.label, i.sha256, i.size, i.filed_by, i.filed_at, i.source, i.keep_until, c.reference, i.file_key";

fn record_json(r: &ortus_core::storage::Row) -> Value {
    json!({
        "id": r.text(0), "reference": r.text(1), "title": r.text(2), "kind": r.text(3), "label": r.text(4),
        "sha256": r.text(5), "size": r.int(6), "filedBy": r.text(7), "filedAt": r.int(8), "source": r.text(9),
        "keepUntil": r.int(10), "case": r.text(11),
    })
}

async fn list(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let all = me.role >= Role::Admin;
    let rows = instance
        .store
        .query(
            &format!(
                "SELECT {COLUMNS} FROM records_items i JOIN records_cases c ON c.id = i.case_id
                 WHERE $1 = 1 OR i.filed_by = $2 OR EXISTS (SELECT 1 FROM records_readers r WHERE r.record_id = i.id AND r.user_id = $2)
                 ORDER BY i.filed_at DESC LIMIT 200"
            ),
            &[i64::from(all).into(), me.id.as_str().into()],
        )
        .await?;
    let list: Vec<Value> = rows.iter().map(record_json).collect();
    Ok(ok(&json!({ "records": list })))
}

async fn one(
    instance: &Instance,
    me: &ortus_core::auth::accounts::User,
    id: &str,
) -> Result<(Value, String), ApiError> {
    if !may_read(instance, me, id).await? {
        return Err(ApiError::not_found("There is no such record."));
    }
    let rows = instance
        .store
        .query(&format!("SELECT {COLUMNS} FROM records_items i JOIN records_cases c ON c.id = i.case_id WHERE i.id = $1"), &[id.into()])
        .await?;
    let r = rows
        .first()
        .ok_or_else(|| ApiError::not_found("There is no such record."))?;
    Ok((record_json(r), r.text(12).unwrap_or_default().to_string()))
}

async fn get(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let id = params.get("record").unwrap_or_default();
    let (mut v, _) = one(instance, &me, id).await?;
    let events: Vec<Value> = instance
        .store
        .query(
            "SELECT at, kind, detail FROM records_events WHERE record_id = $1 ORDER BY at",
            &[id.into()],
        )
        .await?
        .iter()
        .map(|r| json!({"at": r.int(0), "kind": r.text(1), "detail": r.text(2)}))
        .collect();
    v["events"] = json!(events);
    v["people"] = people(instance, v["filedBy"].as_str().map(str::to_string)).await;
    Ok(ok(&v))
}

async fn pdf_of(instance: &Instance, key: &str) -> Result<Vec<u8>, ApiError> {
    instance
        .files
        .get(key)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(|| ApiError::not_found("The record's file is missing."))
}

async fn download(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let (v, key) = one(instance, &me, params.get("record").unwrap_or_default()).await?;
    let bytes = pdf_of(instance, &key).await?;
    let name = format!(
        "{} {}.pdf",
        v["reference"].as_str().unwrap_or("record"),
        v["title"].as_str().unwrap_or_default()
    )
    .chars()
    .filter(|c| !c.is_control() && !matches!(c, '"' | '\\' | '/'))
    .collect::<String>();
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii() { c } else { '_' })
        .collect();
    let h = |n: &str, v: String| rust_web_server::header::Header {
        name: n.to_string().into(),
        value: v.into(),
    };
    let headers = vec![
        h(
            "Content-Disposition",
            format!(
                "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
                ortus_core::form::encode(&name)
            ),
        ),
        h("X-Content-Type-Options", "nosniff".into()),
        h(
            "Content-Security-Policy",
            "sandbox; default-src 'none'".into(),
        ),
        h("Cache-Control", "private, no-store".into()),
    ];
    let body =
        rust_web_server::range::Range::get_content_range(bytes, "application/pdf".to_string());
    let response = Response::get_response(
        STATUS_CODE_REASON_PHRASE.n200_ok,
        Some(headers),
        Some(vec![body]),
    );
    Ok(response)
}

async fn verify(
    request: &Request,
    params: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    let me = signed(request, instance).await?;
    let (v, key) = one(instance, &me, params.get("record").unwrap_or_default()).await?;
    let bytes = pdf_of(instance, &key).await?;
    // The file is the one filed: its fingerprint has not changed.
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let checks = crate::approvals::verify(instance, &bytes)
        .await
        .map_err(|_| ApiError::internal())?;
    Ok(ok(&json!({
        "unchanged": Some(digest.as_str()) == v["sha256"].as_str(),
        "signatures": checks,
    })))
}

macro_rules! handlers {
    ($($name:ident => $f:ident),* $(,)?) => {$(
        fn $name(request: Request, params: PathParams, _: ConnectionInfo, instance: Arc<Instance>) -> BoxFuture<'static, Response> {
            Box::pin(async move { answer(&request, $f(&request, &params, &instance).await) })
        }
    )*};
}

async fn list_p(
    request: &Request,
    _: &PathParams,
    instance: &Arc<Instance>,
) -> Result<Response, ApiError> {
    list(request, instance).await
}

handlers!(h_list => list_p, h_get => get, h_download => download, h_verify => verify);

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
        route(Method::Get, "/records/records", "recordsList", "Records one may read (administrators: all), newest first.", h_list as ortus_core::api::Handler),
        route(Method::Get, "/records/records/:record", "recordsGet", "A record: its case, reference, kind, how long it is kept, and what happened since it was filed.", h_get),
        route(Method::Get, "/records/records/:record/pdf", "recordsDownload", "The filed PDF, as it was filed.", h_download),
        route(Method::Get, "/records/records/:record/verify", "recordsVerify", "Checks the file is unchanged and every signature in it.", h_verify),
    ]
}

/// A person leaves: their records stay (they are the organisation's);
/// they are no longer among the readers.
pub async fn forget(tx: &mut dyn AsyncTx, user: &str) -> StoreResult<()> {
    tx.execute(
        "DELETE FROM records_readers WHERE user_id = $1",
        &[user.into()],
    )
    .await
    .map(|_| ())
}
