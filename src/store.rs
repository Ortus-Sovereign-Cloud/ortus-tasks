//! The tasks store: projects (a name and their members), tasks in a
//! project (title, notes, who does it, when it is due, which column it is
//! in, its place there) and each task's checklist.

use ortus_core::storage::{AsyncStore, AsyncTx, Migration, Row, StoreResult, Value};

use serde_json::{json, Value as Json};

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "projects, tasks, checklists",
    sql: "CREATE TABLE tasks_projects (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        owner_id TEXT NOT NULL,
        created_at BIGINT NOT NULL
    );
    CREATE TABLE tasks_members (
        project_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        PRIMARY KEY (project_id, user_id)
    );
    CREATE INDEX tasks_members_user ON tasks_members (user_id);
    CREATE TABLE tasks_items (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        title TEXT NOT NULL,
        notes TEXT NOT NULL,
        assignee_id TEXT,
        due_at BIGINT,
        state TEXT NOT NULL,
        position BIGINT NOT NULL,
        created_by TEXT NOT NULL,
        created_at BIGINT NOT NULL,
        updated_at BIGINT NOT NULL,
        done_at BIGINT,
        source TEXT
    );
    CREATE INDEX tasks_items_project ON tasks_items (project_id, state, position);
    CREATE INDEX tasks_items_assignee ON tasks_items (assignee_id, state);
    CREATE TABLE tasks_checks (
        task_id TEXT NOT NULL,
        n BIGINT NOT NULL,
        text TEXT NOT NULL,
        done BIGINT NOT NULL,
        PRIMARY KEY (task_id, n)
    )",
}];

/// The columns of a project's board, in order.
pub const STATES: [&str; 4] = ["todo", "doing", "waiting", "done"];

/// Tasks in one project at most; checklist items in one task at most.
pub const MAX_TASKS: i64 = 5000;
pub const MAX_CHECKS: i64 = 100;
pub const MAX_MEMBERS: i64 = 500;

pub fn now() -> i64 {
    ortus_core::program::now_ms()
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}{}", ortus_core::auth::random_hex(12))
}

pub struct Project {
    pub id: String,
    pub name: String,
    pub owner: String,
}

fn project_of(r: &Row) -> Project {
    Project {
        id: r.text(0).map(str::to_string).unwrap_or_default(),
        name: r.text(1).map(str::to_string).unwrap_or_default(),
        owner: r.text(2).map(str::to_string).unwrap_or_default(),
    }
}

/// The projects someone is a member of, by name.
pub async fn projects_of(s: &dyn AsyncStore, user: &str) -> StoreResult<Vec<Project>> {
    Ok(s.query(
        "SELECT p.id, p.name, p.owner_id FROM tasks_projects p
         JOIN tasks_members m ON m.project_id = p.id
         WHERE m.user_id = $1 ORDER BY p.name, p.id",
        &[user.into()],
    )
    .await?
    .iter()
    .map(project_of)
    .collect())
}

/// A project, if the person is one of its members.
pub async fn project(s: &dyn AsyncStore, user: &str, id: &str) -> StoreResult<Option<Project>> {
    Ok(s.query(
        "SELECT p.id, p.name, p.owner_id FROM tasks_projects p
         JOIN tasks_members m ON m.project_id = p.id
         WHERE p.id = $1 AND m.user_id = $2",
        &[id.into(), user.into()],
    )
    .await?
    .first()
    .map(project_of))
}

pub async fn members(s: &dyn AsyncStore, project: &str) -> StoreResult<Vec<String>> {
    Ok(s.query(
        "SELECT user_id FROM tasks_members WHERE project_id = $1 ORDER BY user_id",
        &[project.into()],
    )
    .await?
    .iter()
    .filter_map(|r| r.text(0).map(str::to_string))
    .collect())
}

pub async fn is_member(s: &dyn AsyncStore, project: &str, user: &str) -> StoreResult<bool> {
    Ok(!s
        .query(
            "SELECT 1 FROM tasks_members WHERE project_id = $1 AND user_id = $2",
            &[project.into(), user.into()],
        )
        .await?
        .is_empty())
}

pub async fn add_member(tx: &mut dyn AsyncTx, project: &str, user: &str) -> StoreResult<()> {
    let there = !tx
        .query(
            "SELECT 1 FROM tasks_members WHERE project_id = $1 AND user_id = $2",
            &[project.into(), user.into()],
        )
        .await?
        .is_empty();
    if !there {
        tx.execute(
            "INSERT INTO tasks_members (project_id, user_id) VALUES ($1, $2)",
            &[project.into(), user.into()],
        )
        .await?;
    }
    Ok(())
}

pub async fn create_project(
    s: &dyn AsyncStore,
    owner: &str,
    name: &str,
    others: &[String],
) -> StoreResult<String> {
    let id = new_id("p");
    let mut tx = s.begin().await?;
    tx.execute(
        "INSERT INTO tasks_projects (id, name, owner_id, created_at) VALUES ($1, $2, $3, $4)",
        &[id.as_str().into(), name.into(), owner.into(), now().into()],
    )
    .await?;
    add_member(tx.as_mut(), &id, owner).await?;
    for o in others {
        add_member(tx.as_mut(), &id, o).await?;
    }
    tx.commit().await?;
    Ok(id)
}

pub const TASK_COLUMNS: &str = "id, project_id, title, notes, assignee_id, due_at, state, position, created_by, created_at, updated_at, done_at, source";

pub struct Task {
    pub id: String,
    pub project: String,
    pub title: String,
    pub notes: String,
    pub assignee: Option<String>,
    pub due_at: Option<i64>,
    pub state: String,
    pub position: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub done_at: Option<i64>,
    pub source: Option<String>,
}

pub fn task_of(r: &Row) -> Task {
    Task {
        id: r.text(0).map(str::to_string).unwrap_or_default(),
        project: r.text(1).map(str::to_string).unwrap_or_default(),
        title: r.text(2).map(str::to_string).unwrap_or_default(),
        notes: r.text(3).map(str::to_string).unwrap_or_default(),
        assignee: r.text(4).map(str::to_string),
        due_at: r.int(5),
        state: r.text(6).map(str::to_string).unwrap_or_default(),
        position: r.int(7).unwrap_or(0),
        created_by: r.text(8).map(str::to_string).unwrap_or_default(),
        created_at: r.int(9).unwrap_or(0),
        updated_at: r.int(10).unwrap_or(0),
        done_at: r.int(11),
        source: r.text(12).map(str::to_string),
    }
}

pub async fn task(s: &dyn AsyncStore, id: &str) -> StoreResult<Option<Task>> {
    Ok(s.query(
        &format!("SELECT {TASK_COLUMNS} FROM tasks_items WHERE id = $1"),
        &[id.into()],
    )
    .await?
    .first()
    .map(task_of))
}

pub async fn tasks_in(s: &dyn AsyncStore, project: &str) -> StoreResult<Vec<Task>> {
    Ok(s.query(
        &format!("SELECT {TASK_COLUMNS} FROM tasks_items WHERE project_id = $1 ORDER BY position, created_at"),
        &[project.into()],
    )
    .await?
    .iter()
    .map(task_of)
    .collect())
}

/// Tasks someone does: open ones first by due time, then the done ones
/// of the last two weeks.
pub async fn assigned_to(s: &dyn AsyncStore, user: &str) -> StoreResult<Vec<Task>> {
    Ok(s.query(
        &format!(
            "SELECT {TASK_COLUMNS} FROM tasks_items WHERE assignee_id = $1
             AND (state <> 'done' OR done_at > $2)
             ORDER BY CASE WHEN state = 'done' THEN 1 ELSE 0 END, COALESCE(due_at, 9223372036854775807), created_at"
        ),
        &[user.into(), (now() - 14 * 86_400_000).into()],
    )
    .await?
    .iter()
    .map(task_of)
    .collect())
}

/// The place after the last card of a column.
pub async fn next_position(tx: &mut dyn AsyncTx, project: &str, state: &str) -> StoreResult<i64> {
    Ok(tx
        .query(
            "SELECT CAST(COALESCE(MAX(position), 0) AS BIGINT) FROM tasks_items WHERE project_id = $1 AND state = $2",
            &[project.into(), state.into()],
        )
        .await?
        .first()
        .and_then(|r| r.int(0))
        .unwrap_or(0)
        + 1024)
}

pub async fn count_in(s: &dyn AsyncStore, project: &str) -> StoreResult<i64> {
    Ok(s.query(
        "SELECT COUNT(*) FROM tasks_items WHERE project_id = $1",
        &[project.into()],
    )
    .await?
    .first()
    .and_then(|r| r.int(0))
    .unwrap_or(0))
}

pub struct Check {
    pub n: i64,
    pub text: String,
    pub done: bool,
}

pub async fn checks(s: &dyn AsyncStore, task: &str) -> StoreResult<Vec<Check>> {
    Ok(s.query(
        "SELECT n, text, done FROM tasks_checks WHERE task_id = $1 ORDER BY n",
        &[task.into()],
    )
    .await?
    .iter()
    .map(|r| Check {
        n: r.int(0).unwrap_or(0),
        text: r.text(1).map(str::to_string).unwrap_or_default(),
        done: r.int(2).unwrap_or(0) != 0,
    })
    .collect())
}

/// (done, all) of each task of a project.
pub async fn check_counts(
    s: &dyn AsyncStore,
    project: &str,
) -> StoreResult<std::collections::HashMap<String, (i64, i64)>> {
    Ok(s.query(
        "SELECT c.task_id, CAST(SUM(c.done) AS BIGINT), COUNT(*) FROM tasks_checks c
         JOIN tasks_items t ON t.id = c.task_id WHERE t.project_id = $1 GROUP BY c.task_id",
        &[project.into()],
    )
    .await?
    .iter()
    .map(|r| {
        (
            r.text(0).map(str::to_string).unwrap_or_default(),
            (r.int(1).unwrap_or(0), r.int(2).unwrap_or(0)),
        )
    })
    .collect())
}

pub fn task_json(t: &Task, checks: Option<&[Check]>, counts: Option<(i64, i64)>) -> Json {
    let mut v = json!({
        "id": t.id,
        "project": t.project,
        "title": t.title,
        "notes": t.notes,
        "assignee": t.assignee,
        "dueAt": t.due_at,
        "state": t.state,
        "position": t.position,
        "createdBy": t.created_by,
        "createdAt": t.created_at,
        "updatedAt": t.updated_at,
        "doneAt": t.done_at,
        "source": t.source,
    });
    if let Some(c) = checks {
        v["checks"] = c
            .iter()
            .map(|c| json!({"n": c.n, "text": c.text, "done": c.done}))
            .collect();
        v["checked"] = json!([c.iter().filter(|c| c.done).count(), c.len()]);
    } else if let Some((done, all)) = counts {
        v["checked"] = json!([done, all]);
    }
    v
}

/// A person leaves: their memberships go, tasks they did are nobody's,
/// projects with no one left go with their tasks; projects they owned
/// pass to another member.
pub async fn forget(tx: &mut dyn AsyncTx, user: &str) -> StoreResult<()> {
    let owned: Vec<String> = tx
        .query(
            "SELECT id FROM tasks_projects WHERE owner_id = $1",
            &[user.into()],
        )
        .await?
        .iter()
        .filter_map(|r| r.text(0).map(str::to_string))
        .collect();
    tx.execute(
        "DELETE FROM tasks_members WHERE user_id = $1",
        &[user.into()],
    )
    .await?;
    tx.execute(
        "UPDATE tasks_items SET assignee_id = NULL WHERE assignee_id = $1",
        &[user.into()],
    )
    .await?;
    for p in owned {
        let next = tx
            .query(
                "SELECT user_id FROM tasks_members WHERE project_id = $1 ORDER BY user_id",
                &[p.as_str().into()],
            )
            .await?
            .first()
            .and_then(|r| r.text(0).map(str::to_string));
        match next {
            Some(n) => {
                tx.execute(
                    "UPDATE tasks_projects SET owner_id = $1 WHERE id = $2",
                    &[n.as_str().into(), p.as_str().into()],
                )
                .await?;
            }
            None => {
                let ids: Vec<String> = tx
                    .query(
                        "SELECT id FROM tasks_items WHERE project_id = $1",
                        &[p.as_str().into()],
                    )
                    .await?
                    .iter()
                    .filter_map(|r| r.text(0).map(str::to_string))
                    .collect();
                // Their search entries are the person's: the core forgets them.
                for id in ids {
                    tx.execute(
                        "DELETE FROM tasks_checks WHERE task_id = $1",
                        &[id.as_str().into()],
                    )
                    .await?;
                }
                tx.execute(
                    "DELETE FROM tasks_items WHERE project_id = $1",
                    &[p.as_str().into()],
                )
                .await?;
                tx.execute(
                    "DELETE FROM tasks_projects WHERE id = $1",
                    &[p.as_str().into()],
                )
                .await?;
            }
        }
    }
    Ok(())
}

pub fn value_opt_text(v: Option<&str>) -> Value {
    match v {
        Some(t) => t.into(),
        None => Value::Null,
    }
}

pub fn value_opt_int(v: Option<i64>) -> Value {
    match v {
        Some(t) => t.into(),
        None => Value::Null,
    }
}
