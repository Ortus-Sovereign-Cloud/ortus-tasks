//! The approvals app's part in signing: the `Signing` service (sign with
//! the person's own certificate, check every signature in a PDF) and the
//! organisation's authority certificate for anyone who checks.

use std::sync::{Arc, Weak};

use ortus_core::api::{answer, ApiError, Method, Route};
use ortus_core::auth::Access;
use ortus_core::instance::Instance;
use ortus_core::interfaces::SignatureCheck;
use ortus_core::storage::{BoxFuture, Migration};
use rust_web_server::prelude::*;

use crate::{certs, pades};

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "signing authority and personal signing keys",
    sql: "CREATE TABLE signing_ca (
        id TEXT PRIMARY KEY,
        cert {{blob}} NOT NULL,
        key_sealed TEXT NOT NULL,
        created_at BIGINT NOT NULL
    );
    CREATE TABLE signing_keys (
        user_id TEXT NOT NULL,
        serial TEXT PRIMARY KEY,
        cert {{blob}} NOT NULL,
        key_sealed TEXT NOT NULL,
        created_at BIGINT NOT NULL,
        revoked_at BIGINT
    );
    CREATE INDEX signing_keys_user ON signing_keys (user_id, revoked_at)",
}];

/// Signs as `user`: their certificate (made the first time), the
/// authority's certificate with it, the time and the reason.
pub async fn sign(
    instance: &Instance,
    user: &str,
    pdf: &[u8],
    reason: &str,
) -> Result<Vec<u8>, String> {
    let person = ortus_core::auth::accounts::by_id(instance.store.as_ref(), user)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no such person")?;
    let signer = certs::personal(instance, &person).await?;
    let ca = certs::authority_cert(instance).await?;
    let signed = pades::sign(
        pdf,
        &signer,
        &[ca],
        &pades::Signing {
            name: &person.name,
            reason,
            at_ms: ortus_core::program::now_ms(),
        },
    )?;
    ortus_core::auth::audit(
        instance,
        ortus_core::audit::Event::new("signing.signed", ortus_core::audit::Outcome::Done)
            .by(user)
            .with(serde_json::json!({"reason": reason})),
    )
    .await;
    Ok(signed)
}

/// Every signature in a PDF, checked; revoked certificates say since when.
pub async fn verify(instance: &Instance, pdf: &[u8]) -> Result<Vec<SignatureCheck>, String> {
    let ca = certs::authority_cert(instance).await?;
    let mut out = Vec::new();
    for f in pades::verify(pdf, &ca) {
        out.push(SignatureCheck {
            revoked_at: certs::revoked_at(instance.store.as_ref(), &f.cert).await,
            signer: f.signer,
            email: f.email,
            at: f.at,
            reason: f.reason,
            intact: f.intact,
            trusted: f.trusted,
            whole: f.whole,
        });
    }
    Ok(out)
}

pub struct Service {
    pub instance: Weak<Instance>,
}

impl ortus_core::interfaces::Signing for Service {
    fn sign<'a>(
        &'a self,
        user: &'a str,
        pdf: &'a [u8],
        reason: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        Box::pin(async move {
            let instance = self.instance.upgrade().ok_or("stopping")?;
            sign(&instance, user, pdf, reason).await
        })
    }

    fn verify<'a>(&'a self, pdf: &'a [u8]) -> BoxFuture<'a, Result<Vec<SignatureCheck>, String>> {
        Box::pin(async move {
            let instance = self.instance.upgrade().ok_or("stopping")?;
            verify(&instance, pdf).await
        })
    }
}

async fn authority(request: &Request, instance: &Arc<Instance>) -> Result<Response, ApiError> {
    let _ = request;
    let cert = certs::authority_cert(instance)
        .await
        .map_err(|_| ApiError::internal())?;
    let body = rust_web_server::range::Range::get_content_range(
        certs::pem(&cert).into_bytes(),
        "application/x-pem-file".to_string(),
    );
    let headers = vec![rust_web_server::header::Header {
        name: "Content-Disposition".into(),
        value: "attachment; filename=\"signing-authority.pem\"".into(),
    }];
    Ok(Response::get_response(
        STATUS_CODE_REASON_PHRASE.n200_ok,
        Some(headers),
        Some(vec![body]),
    ))
}

fn h_authority(
    request: Request,
    _: PathParams,
    _: ConnectionInfo,
    instance: Arc<Instance>,
) -> BoxFuture<'static, Response> {
    Box::pin(async move { answer(&request, authority(&request, &instance).await) })
}

pub fn routes() -> Vec<Route> {
    vec![Route {
        method: Method::Get,
        path: "/approvals/authority.pem",
        id: "approvalsAuthority",
        tag: "approvals",
        summary: "The organisation's signing authority certificate (PEM), to check signatures with any PDF reader.",
        access: Access::Public,
        response: crate::api::object,
        handler: h_authority,
    }]
}

/// A person leaves: their signing certificates are revoked (signatures
/// they made stay valid; new ones cannot be made).
pub async fn forget(tx: &mut dyn ortus_core::storage::AsyncTx, user: &str) -> Result<(), String> {
    tx.execute(
        "UPDATE signing_keys SET revoked_at = $1 WHERE user_id = $2 AND revoked_at IS NULL",
        &[ortus_core::program::now_ms().into(), user.into()],
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}
