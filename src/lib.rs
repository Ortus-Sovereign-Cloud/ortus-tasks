//! Tasks and approvals: tasks, approvals, signing, records and read-and-confirm.
//!
//! An Ortus app crate: it depends on `ortus-core` only and hands its apps
//! to a program through [`apps`].

pub mod api;
pub mod certs;
pub mod index;
pub mod pades;
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

/// The approvals app.
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
}

/// The apps of this crate, for a program to run.
pub fn apps() -> Vec<Arc<dyn App>> {
    vec![Arc::new(Tasks), Arc::new(Approvals)]
}
