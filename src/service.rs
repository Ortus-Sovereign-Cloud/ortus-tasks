//! The `Tasks` service other apps use to make a task for someone (a call's
//! action item, a decision in chat): it goes into the creator's project
//! named "Action items" (made the first time), the assignee added to it.

use std::sync::Weak;

use ortus_core::instance::Instance;
use ortus_core::interfaces::ItemRef;
use ortus_core::storage::BoxFuture;

use crate::store;

/// The project other apps' tasks go into.
pub const ACTION_ITEMS: &str = "Action items";

pub struct Service {
    pub instance: Weak<Instance>,
}

impl ortus_core::interfaces::Tasks for Service {
    fn create<'a>(
        &'a self,
        user: &'a str,
        title: &'a str,
        assignee: &'a str,
        due_ms: Option<i64>,
        from: Option<ItemRef>,
    ) -> BoxFuture<'a, Result<ItemRef, String>> {
        Box::pin(async move {
            let instance = self.instance.upgrade().ok_or("stopping")?;
            let s = instance.store.as_ref();
            let me = ortus_core::auth::accounts::by_id(s, user)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("no such person")?;
            // The assignee by user id (or address).
            let who = if assignee.contains('@') {
                ortus_core::auth::accounts::by_email(s, &assignee.to_ascii_lowercase())
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                ortus_core::auth::accounts::by_id(s, assignee)
                    .await
                    .map_err(|e| e.to_string())?
            }
            .ok_or("the assignee is not a colleague")?;
            let title: String = title
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(300)
                .collect();
            if title.is_empty() {
                return Err("a task has a title".into());
            }
            let project = match store::projects_of(s, &me.id)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|p| p.name == ACTION_ITEMS && p.owner == me.id)
            {
                Some(p) => p.id,
                None => store::create_project(s, &me.id, ACTION_ITEMS, &[])
                    .await
                    .map_err(|e| e.to_string())?,
            };
            let source = from.map(|f| f.path).filter(|p| crate::api::local_path(p));
            let id = crate::api::insert_task(
                &instance,
                &me,
                &project,
                &title,
                "",
                Some(&who.id),
                due_ms,
                "todo",
                source.as_deref(),
                &[],
            )
            .await
            .map_err(|e| e.message)?;
            Ok(ItemRef {
                app: "tasks".into(),
                id: id.clone(),
                path: format!("/tasks?task={id}"),
            })
        })
    }
}
