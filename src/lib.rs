//! Tasks and approvals: tasks, approvals, signing, records and read-and-confirm.
//!
//! An Ortus app crate: it depends on `ortus-core` only and hands its apps
//! to a program through [`apps`].

use std::sync::Arc;

use ortus_core::app::{App, AppInfo, Group};

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
