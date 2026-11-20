//! The core with this crate's apps, for development and the browser tests
//! (`cargo run --example dev`). The released programs are built in the
//! `ortus` repository. For the tests only, `POST /api/v1/records/dev-sample`
//! signs a sample PDF as the caller and files it (docs does that in the
//! programs).

use std::sync::Arc;

use ortus_core::api::{answer, ok, ApiError, Method, Route};
use ortus_core::app::{App, AppInfo};
use ortus_core::auth::Access;
use ortus_core::instance::Instance;
use ortus_core::interfaces::{NewRecord, Records, Signing};
use ortus_core::storage::BoxFuture;
use rust_web_server::prelude::*;

fn sample_pdf() -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let stream = "BT /F1 24 Tf 72 740 Td (Flood response plan 2026) Tj ET BT /F1 12 Tf 72 700 Td (Approved for use by all districts.) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".to_string(),
    ];
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

async fn file_sample(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let me = ortus_core::auth::current(request, instance)
        .await
        .ok_or_else(ApiError::sign_in)?
        .user;
    let signing = instance
        .services
        .get::<dyn Signing>(&instance.switches)
        .await
        .map_err(|_| ApiError::internal())?;
    let records = instance
        .services
        .get::<dyn Records>(&instance.switches)
        .await
        .map_err(|_| ApiError::internal())?;
    let signed = signing
        .sign(&me.id, &sample_pdf(), "Author signs")
        .await
        .map_err(|_| ApiError::internal())?;
    let readers = vec![me.id.clone()];
    let filed = records
        .file(NewRecord {
            by: &me.id,
            title: "Flood response plan 2026",
            kind: "decision",
            case: Some("2026/EM/0114"),
            label: Some("official-sensitive"),
            pdf: &signed,
            source: "/docs",
            readers: &readers,
        })
        .await
        .map_err(|_| ApiError::internal())?;
    Ok(ok(
        &serde_json::json!({ "id": filed.id, "path": filed.path }),
    ))
}

fn h_file(
    request: Request,
    _: PathParams,
    _: ConnectionInfo,
    instance: Arc<Instance>,
) -> BoxFuture<'static, Response> {
    Box::pin(async move { answer(&request, file_sample(&request, &instance).await) })
}

/// The records app with the test route added (no app of its own: the
/// shell names only apps it knows).
struct DevRecords(ortus_tasks::Records);

impl App for DevRecords {
    fn info(&self) -> AppInfo {
        self.0.info()
    }
    fn migrations(&self) -> &'static [ortus_core::storage::Migration] {
        self.0.migrations()
    }
    fn routes(&self) -> Vec<Route> {
        let mut all = self.0.routes();
        all.push(Route {
            method: Method::Post,
            path: "/records/dev-sample",
            id: "devRecord",
            tag: "dev",
            summary: "Development only: files a signed sample PDF.",
            access: Access::Signed,
            response: || serde_json::json!({"type": "object"}),
            handler: h_file,
        });
        all
    }
    fn provide(&self, instance: &Arc<Instance>, services: &ortus_core::services::Services) {
        self.0.provide(instance, services)
    }
    fn web_file(&self, path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
        self.0.web_file(path)
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut apps: Vec<Arc<dyn App>> = ortus_tasks::apps()
        .into_iter()
        .filter(|a| a.info().id != "records")
        .collect();
    apps.push(Arc::new(DevRecords(ortus_tasks::Records)));
    let status = ortus_core::cli::run("ortus-tasks-dev", ortus_core::VERSION, apps, &args).await;
    std::process::exit(status);
}
