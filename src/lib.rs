//! Tasks and approvals: tasks, approvals, signing, records and read-and-confirm.
//!
//! An Ortus app crate: it depends on `ortus-core` only and hands its apps
//! to a program through [`apps`].

pub mod api;
pub mod approvals;
pub mod certs;
pub mod index;
pub mod mustread;
pub mod pades;
pub mod records;
pub mod store;

use std::sync::Arc;

use ortus_core::app::{App, AppInfo, Group};
use ortus_core::instance::Instance;
use ortus_core::storage::{AsyncTx, BoxFuture, Migration};

/// The built web client (web/dist).
#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
struct Web;

/// The tasks app.
pub struct Tasks;

impl App for Tasks {
    fn info(&self) -> AppInfo {
        AppInfo {
            id: "tasks",
            group: Group::Organise,
            path: "/tasks",
            order: 2,
        }
    }

    fn migrations(&self) -> &'static [Migration] {
        store::MIGRATIONS
    }

    fn routes(&self) -> Vec<ortus_core::api::Route> {
        api::routes()
    }

    fn search_owners<'a>(
        &'a self,
        instance: &'a Instance,
        user: &'a str,
    ) -> BoxFuture<'a, Vec<String>> {
        Box::pin(index::owners_for(instance, user))
    }

    fn search_allowed<'a>(
        &'a self,
        instance: &'a Instance,
        user: &'a str,
        ids: Vec<String>,
    ) -> BoxFuture<'a, Vec<String>> {
        Box::pin(index::visible(instance, user, ids))
    }

    fn forget_user<'a>(
        &'a self,
        tx: &'a mut dyn AsyncTx,
        user_id: &'a str,
        _rules: &'a ortus_core::retention::Rules,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { store::forget(tx, user_id).await.map_err(|e| e.to_string()) })
    }

    fn web_file(&self, path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
        Web::get(path).map(|file| file.data)
    }
}

/// The approvals app: what waits for one's decision or signature, and
/// signing (the `Signing` service).
pub struct Approvals;

impl App for Approvals {
    fn info(&self) -> AppInfo {
        AppInfo {
            id: "approvals",
            group: Group::Create,
            path: "/approvals",
            order: 3,
        }
    }

    fn migrations(&self) -> &'static [Migration] {
        approvals::MIGRATIONS
    }

    fn routes(&self) -> Vec<ortus_core::api::Route> {
        approvals::routes()
    }

    fn provide(&self, instance: &Arc<Instance>, services: &ortus_core::services::Services) {
        let signing: Arc<dyn ortus_core::interfaces::Signing> = Arc::new(approvals::Service {
            instance: Arc::downgrade(instance),
        });
        services.provide("approvals", signing);
    }

    fn forget_user<'a>(
        &'a self,
        tx: &'a mut dyn AsyncTx,
        user_id: &'a str,
        _rules: &'a ortus_core::retention::Rules,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(approvals::forget(tx, user_id))
    }

    // This app's entry is approvals.js; the rest is shared.
    fn web_file(&self, path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
        Web::get(if path == "app.js" {
            "approvals.js"
        } else {
            path
        })
        .map(|file| file.data)
    }
}

/// The records app: cases and the records filed to them (the `Records`
/// service).
pub struct Records;

impl App for Records {
    fn info(&self) -> AppInfo {
        AppInfo {
            id: "records",
            group: Group::Organise,
            path: "/records",
            order: 4,
        }
    }

    fn migrations(&self) -> &'static [Migration] {
        records::MIGRATIONS
    }

    fn routes(&self) -> Vec<ortus_core::api::Route> {
        let mut all = records::routes();
        all.extend(mustread::routes());
        all
    }

    fn provide(&self, instance: &Arc<Instance>, services: &ortus_core::services::Services) {
        let records: Arc<dyn ortus_core::interfaces::Records> = Arc::new(records::Service {
            instance: Arc::downgrade(instance),
        });
        services.provide("records", records);
    }

    fn forget_user<'a>(
        &'a self,
        tx: &'a mut dyn AsyncTx,
        user_id: &'a str,
        _rules: &'a ortus_core::retention::Rules,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            records::forget(tx, user_id)
                .await
                .map_err(|e| e.to_string())?;
            mustread::forget(tx, user_id)
                .await
                .map_err(|e| e.to_string())
        })
    }

    // This app's entry is records.js; the rest is shared.
    fn web_file(&self, path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
        Web::get(if path == "app.js" { "records.js" } else { path }).map(|file| file.data)
    }
}

/// The apps of this crate, for a program to run.
pub fn apps() -> Vec<Arc<dyn App>> {
    vec![Arc::new(Tasks), Arc::new(Approvals), Arc::new(Records)]
}
