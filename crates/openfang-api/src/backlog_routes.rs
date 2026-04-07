//! `/api/projects/{id}/backlog/*` — task CRUD via [`openfang_kernel::backlog_store::BacklogStore`].

use super::{parse_project_id_param, AppState};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use openfang_kernel::backlog_store::BacklogStoreError;
use openfang_types::backlog::reader::build_doc_tree;
use openfang_types::backlog::{
    AcceptanceCriterion, BacklogDecision, BacklogDocument, BacklogMilestone, BacklogSearchResult,
    BacklogSnapshot, BacklogTask, DecisionStatus, DocTreeNode, TaskPriority,
};
use openfang_types::project::ProjectId;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

fn deserialize_one_or_many_strings<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StrVec;
    impl<'de> Visitor<'de> for StrVec {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or list of strings")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(vec![v.to_string()])
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
            Ok(vec![v])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                out.push(s);
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(StrVec)
}

#[derive(Debug, Deserialize)]
pub struct BacklogTasksCleanupQuery {
    /// Days: tasks with reference date before `today - age` are eligible (Backlog.md cleanup).
    pub age: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogTasksCleanupExecuteBody {
    pub age: u32,
}

#[derive(Debug, Deserialize)]
pub struct BacklogTasksListQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBacklogTaskBody {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<TaskPriority>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub assignee: Option<Vec<String>>,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogTaskPatchBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<TaskPriority>,
    #[serde(default)]
    pub assignee: Option<Vec<String>>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub ordinal: Option<i32>,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
    /// Set to milestone id (e.g. `MS-1`) or empty string to clear.
    #[serde(default)]
    pub milestone: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, alias = "acceptanceCriteriaItems")]
    pub acceptance_criteria: Option<Vec<AcceptanceCriterion>>,
    #[serde(default)]
    pub implementation_plan: Option<String>,
    #[serde(default)]
    pub implementation_notes: Option<String>,
    #[serde(default)]
    pub final_summary: Option<String>,
    /// Toggle AC line at index (0-based within `<!-- AC:BEGIN -->`); exclusive of other patch fields in one request.
    #[serde(default)]
    pub toggle_ac: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderBacklogTasksBody {
    pub task_id: String,
    pub target_status: String,
    pub ordered_task_ids: Vec<String>,
}

fn map_backlog_err(e: BacklogStoreError) -> (StatusCode, Json<serde_json::Value>) {
    match e {
        BacklogStoreError::NotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Not found"})),
        ),
        BacklogStoreError::ProjectNotFound => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project not found"})),
        ),
        BacklogStoreError::Msg(m) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": m})),
        ),
        other => {
            tracing::warn!(%other, "backlog store error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Backlog operation failed"})),
            )
        }
    }
}

fn ensure_project(
    state: &AppState,
    pid: ProjectId,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if state.kernel.project_store.get(pid).is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Project not found"})),
        ));
    }
    state
        .backlog_store
        .ensure_loaded(&pid, &state.kernel.project_store)
        .map_err(map_backlog_err)?;
    if let Some(p) = state.kernel.project_store.get(pid) {
        state.backlog_watcher.ensure_watching(
            pid,
            p.backlog_root(),
            state.backlog_store.clone(),
        );
    }
    Ok(())
}

fn task_matches_query(t: &BacklogTask, q: &BacklogTasksListQuery) -> bool {
    if let Some(ref s) = q.status {
        if !t.status.eq_ignore_ascii_case(s.trim()) {
            return false;
        }
    }
    if let Some(ref p) = q.priority {
        let want = p.trim().to_ascii_lowercase();
        match &t.priority {
            Some(pr) => {
                let got = match pr {
                    TaskPriority::High => "high",
                    TaskPriority::Medium => "medium",
                    TaskPriority::Low => "low",
                };
                if got != want {
                    return false;
                }
            }
            None => return false,
        }
    }
    if let Some(ref lab) = q.label {
        let l = lab.to_ascii_lowercase();
        if !t
            .labels
            .iter()
            .any(|x| x.to_ascii_lowercase().contains(&l))
        {
            return false;
        }
    }
    if let Some(ref a) = q.assignee {
        let want = a.to_ascii_lowercase();
        if !t
            .assignee
            .iter()
            .any(|x| x.to_ascii_lowercase().contains(&want))
        {
            return false;
        }
    }
    true
}

fn apply_patch(task: &mut BacklogTask, p: &BacklogTaskPatchBody) {
    if let Some(t) = &p.title {
        let t = t.trim();
        if !t.is_empty() {
            task.title = t.to_string();
        }
    }
    if let Some(s) = &p.status {
        task.status.clone_from(s);
    }
    if let Some(pr) = p.priority {
        task.priority = Some(pr);
    }
    if let Some(a) = &p.assignee {
        task.assignee.clone_from(a);
    }
    if let Some(l) = &p.labels {
        task.labels.clone_from(l);
    }
    if let Some(o) = p.ordinal {
        task.ordinal = Some(o);
    }
    if let Some(d) = &p.dependencies {
        task.dependencies.clone_from(d);
    }
    if let Some(m) = &p.milestone {
        let m = m.trim();
        task.milestone = if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        };
    }
    if let Some(d) = &p.description {
        task.description = Some(d.clone());
    }
    if let Some(ac) = &p.acceptance_criteria {
        task.acceptance_criteria = ac
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut x = c.clone();
                x.index = i;
                x
            })
            .collect();
    }
    if let Some(v) = &p.implementation_plan {
        task.implementation_plan = Some(v.clone());
    }
    if let Some(v) = &p.implementation_notes {
        task.implementation_notes = Some(v.clone());
    }
    if let Some(v) = &p.final_summary {
        task.final_summary = Some(v.clone());
    }
}

/// GET /api/projects/:id/backlog/config
pub async fn backlog_get_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    match serde_json::to_value(&snap.config) {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => {
            tracing::error!("serialize backlog config: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Serialize failed"})),
            )
                .into_response()
        }
    }
}

/// GET /api/projects/:id/backlog/tasks
pub async fn backlog_list_tasks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<BacklogTasksListQuery>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let mut rows: Vec<serde_json::Value> = snap
        .tasks
        .iter()
        .filter(|t| task_matches_query(t, &q))
        .filter_map(|t| serde_json::to_value(t).ok())
        .collect();
    rows.sort_by(|a, b| {
        let ia = a["id"].as_str().unwrap_or("");
        let ib = b["id"].as_str().unwrap_or("");
        ia.cmp(ib)
    });
    Json(rows).into_response()
}

/// GET /api/projects/:id/backlog/tasks/cleanup?age=N — preview Done tasks under `tasks/` older than N days (Backlog.md-compatible).
pub async fn backlog_cleanup_tasks_preview(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<BacklogTasksCleanupQuery>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let age: u32 = match q.age.trim().parse() {
        Ok(a) if !q.age.trim().is_empty() => a,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing or invalid age parameter"})),
            )
                .into_response();
        }
    };
    match state.backlog_store.cleanup_done_tasks_preview(&pid, age) {
        Ok(rows) => {
            let tasks: Vec<serde_json::Value> = rows
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "title": c.title,
                        "createdDate": c.created_date,
                        "updatedDate": c.updated_date,
                    })
                })
                .collect();
            Json(serde_json::json!({
                "count": tasks.len(),
                "tasks": tasks,
            }))
            .into_response()
        }
        Err(BacklogStoreError::NotLoaded) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// POST /api/projects/:id/backlog/tasks/cleanup/execute — move matching Done tasks to `completed/`.
pub async fn backlog_cleanup_tasks_execute(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<BacklogTasksCleanupExecuteBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state.backlog_store.cleanup_done_tasks_execute(
        &pid,
        body.age,
        &state.kernel.project_store,
    ) {
        Ok((moved, total, failed)) => {
            let message = if total == 0 {
                "No tasks to clean up".to_string()
            } else {
                format!("Moved {moved} of {total} tasks to completed folder")
            };
            let mut resp = serde_json::json!({
                "success": true,
                "movedCount": moved,
                "totalCount": total,
                "message": message,
            });
            if !failed.is_empty() {
                if let Some(obj) = resp.as_object_mut() {
                    obj.insert(
                        "failedTasks".into(),
                        serde_json::to_value(&failed).unwrap_or(serde_json::Value::Null),
                    );
                }
            }
            Json(resp).into_response()
        }
        Err(BacklogStoreError::NotLoaded) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// GET /api/projects/:id/backlog/tasks/:task_id
pub async fn backlog_get_task(
    State(state): State<Arc<AppState>>,
    Path((id, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state.backlog_store.get_active_task(&pid, &task_id) {
        Some(t) => match serde_json::to_value(&t) {
            Ok(v) => (StatusCode::OK, Json(v)).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Serialize failed"})),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Task not found"})),
        )
            .into_response(),
    }
}

/// POST /api/projects/:id/backlog/tasks
pub async fn backlog_create_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<CreateBacklogTaskBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if body.title.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "title must not be empty"})),
        )
            .into_response();
    }
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let default_status = state
        .backlog_store
        .get_snapshot(&pid)
        .map(|s| {
            let d = s.config.default_status.trim();
            if d.is_empty() {
                "Open".to_string()
            } else {
                d.to_string()
            }
        })
        .unwrap_or_else(|| "Open".to_string());

    let task = BacklogTask {
        title: body.title.trim().to_string(),
        status: body
            .status
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(default_status),
        created_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        description: body.description,
        priority: body.priority,
        labels: body.labels.unwrap_or_default(),
        assignee: body.assignee.unwrap_or_default(),
        dependencies: body.dependencies.unwrap_or_default(),
        ..Default::default()
    };

    match state
        .backlog_store
        .create_task(&pid, task, &state.kernel.project_store)
    {
        Ok(t) => match serde_json::to_value(&t) {
            Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Serialize failed"})),
            )
                .into_response(),
        },
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// PUT /api/projects/:id/backlog/tasks/:task_id
pub async fn backlog_put_task(
    State(state): State<Arc<AppState>>,
    Path((id, task_id)): Path<(String, String)>,
    Json(patch): Json<BacklogTaskPatchBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    if let Some(idx) = patch.toggle_ac {
        return match state
            .backlog_store
            .toggle_ac(&pid, &task_id, idx, &state.kernel.project_store)
        {
            Ok(()) => {
                let fresh = match state.backlog_store.get_active_task(&pid, &task_id) {
                    Some(t) => t,
                    None => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Task missing after toggle"})),
                        )
                            .into_response();
                    }
                };
                match serde_json::to_value(&fresh) {
                    Ok(v) => (StatusCode::OK, Json(v)).into_response(),
                    Err(_) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Serialize failed"})),
                    )
                        .into_response(),
                }
            }
            Err(e) => map_backlog_err(e).into_response(),
        };
    }

    let mut task = match state.backlog_store.get_active_task(&pid, &task_id) {
        Some(t) => t,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Task not found"})),
            )
                .into_response();
        }
    };
    apply_patch(&mut task, &patch);
    match state
        .backlog_store
        .write_active_task(&pid, &task, &state.kernel.project_store)
    {
        Ok(()) => {
            let fresh = state
                .backlog_store
                .get_active_task(&pid, &task_id)
                .expect("task after write");
            match serde_json::to_value(&fresh) {
                Ok(v) => (StatusCode::OK, Json(v)).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Serialize failed"})),
                )
                    .into_response(),
            }
        }
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// DELETE /api/projects/:id/backlog/tasks/:task_id
pub async fn backlog_delete_task(
    State(state): State<Arc<AppState>>,
    Path((id, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state
        .backlog_store
        .archive_task(&pid, &task_id, &state.kernel.project_store)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// POST /api/projects/:id/backlog/tasks/:task_id/complete
pub async fn backlog_complete_task(
    State(state): State<Arc<AppState>>,
    Path((id, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state
        .backlog_store
        .complete_task(&pid, &task_id, &state.kernel.project_store)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// POST /api/projects/:id/backlog/tasks/reorder
pub async fn backlog_reorder_tasks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ReorderBacklogTasksBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    if body.ordered_task_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "ordered_task_ids must not be empty"})),
        )
            .into_response();
    }
    match state.backlog_store.reorder_tasks_in_column(
        &pid,
        &body.task_id,
        &body.target_status,
        &body.ordered_task_ids,
        &state.kernel.project_store,
    ) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

fn doc_entry_for_tree(d: &BacklogDocument) -> serde_json::Value {
    serde_json::json!({
        "id": d.id,
        "title": d.title,
        "type": d.doc_type,
        "path": d.path.as_deref().unwrap_or(""),
    })
}

fn doc_tree_node_to_json(n: &DocTreeNode) -> serde_json::Value {
    serde_json::json!({
        "name": n.name,
        "children": n.children.iter().map(doc_tree_node_to_json).collect::<Vec<_>>(),
        "docs": n.docs.iter().map(doc_entry_for_tree).collect::<Vec<_>>(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBacklogDocBody {
    pub title: String,
    #[serde(default, rename = "type")]
    pub doc_type: Option<String>,
    #[serde(default)]
    pub category_path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBacklogDocBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// GET /api/projects/:id/backlog/docs — nested doc tree for sidebar nav.
pub async fn backlog_list_docs_tree(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let tree = build_doc_tree(&snap.documents);
    Json(doc_tree_node_to_json(&tree.root)).into_response()
}

/// GET /api/projects/:id/backlog/docs/:doc_id
pub async fn backlog_get_doc(
    State(state): State<Arc<AppState>>,
    Path((id, doc_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state.backlog_store.get_document(&pid, &doc_id) {
        Some(d) => match serde_json::to_value(&d) {
            Ok(v) => (StatusCode::OK, Json(v)).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Serialize failed"})),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Not found"})),
        )
            .into_response(),
    }
}

/// POST /api/projects/:id/backlog/docs
pub async fn backlog_create_doc(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<CreateBacklogDocBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if body.title.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "title must not be empty"})),
        )
            .into_response();
    }
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let dt = body.doc_type.as_deref().unwrap_or("guide");
    match state.backlog_store.create_document(
        &pid,
        body.title.trim(),
        dt,
        body.category_path.trim(),
        &body.content,
        &state.kernel.project_store,
    ) {
        Ok(d) => match serde_json::to_value(&d) {
            Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Serialize failed"})),
            )
                .into_response(),
        },
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// PUT /api/projects/:id/backlog/docs/:doc_id
pub async fn backlog_update_doc(
    State(state): State<Arc<AppState>>,
    Path((id, doc_id)): Path<(String, String)>,
    Json(body): Json<UpdateBacklogDocBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let mut doc = match state.backlog_store.get_document(&pid, &doc_id) {
        Some(d) => d,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Not found"})),
            )
                .into_response();
        }
    };
    if let Some(t) = body.title {
        if !t.trim().is_empty() {
            doc.title = t;
        }
    }
    if let Some(c) = body.content {
        doc.raw_content = c;
    }
    doc.updated_date = Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
    doc.file_path = None;
    match state
        .backlog_store
        .update_doc(&pid, &doc, &state.kernel.project_store)
    {
        Ok(()) => {
            let fresh = state
                .backlog_store
                .get_document(&pid, &doc_id)
                .expect("doc after update");
            match serde_json::to_value(&fresh) {
                Ok(v) => (StatusCode::OK, Json(v)).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Serialize failed"})),
                )
                    .into_response(),
            }
        }
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// DELETE /api/projects/:id/backlog/docs/:doc_id
pub async fn backlog_delete_doc(
    State(state): State<Arc<AppState>>,
    Path((id, doc_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state
        .backlog_store
        .delete_document(&pid, &doc_id, &state.kernel.project_store)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

fn task_milestone_matches(task: &BacklogTask, milestone_id: &str) -> bool {
    task.milestone
        .as_deref()
        .map(|m| m.trim() == milestone_id)
        .unwrap_or(false)
}

fn task_status_done_like(t: &BacklogTask) -> bool {
    matches!(
        t.status.trim().to_ascii_lowercase().as_str(),
        "done" | "closed" | "complete" | "completed"
    )
}

fn milestone_progress(snap: &BacklogSnapshot, milestone_id: &str) -> (u32, u32, f64) {
    let mut total = 0u32;
    let mut done = 0u32;
    for t in &snap.tasks {
        if task_milestone_matches(t, milestone_id) {
            total += 1;
            if task_status_done_like(t) {
                done += 1;
            }
        }
    }
    for t in &snap.completed {
        if task_milestone_matches(t, milestone_id) {
            total += 1;
            done += 1;
        }
    }
    let pct = if total == 0 {
        0.0
    } else {
        (f64::from(done) / f64::from(total)) * 100.0
    };
    (total, done, pct)
}

fn milestone_with_stats_json(snap: &BacklogSnapshot, m: &BacklogMilestone) -> serde_json::Value {
    let (task_total, task_done, progress_percent) = milestone_progress(snap, &m.id);
    let mut v = serde_json::to_value(m).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.insert("taskTotal".into(), task_total.into());
        obj.insert("taskDone".into(), task_done.into());
        obj.insert(
            "progressPercent".into(),
            serde_json::json!(progress_percent),
        );
    }
    v
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBacklogDecisionBody {
    pub title: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBacklogDecisionBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub consequences: Option<String>,
    #[serde(default)]
    pub alternatives: Option<String>,
    #[serde(default)]
    pub status: Option<DecisionStatus>,
}

fn apply_decision_update(d: &mut BacklogDecision, body: &UpdateBacklogDecisionBody) {
    if let Some(ref t) = body.title {
        if !t.trim().is_empty() {
            d.title.clone_from(t);
        }
    }
    if let Some(ref c) = body.context {
        d.context.clone_from(c);
    }
    if let Some(ref x) = body.decision {
        d.decision.clone_from(x);
    }
    if let Some(ref c) = body.consequences {
        d.consequences.clone_from(c);
    }
    if let Some(ref a) = body.alternatives {
        d.alternatives = Some(a.clone());
    }
    if let Some(s) = body.status {
        d.status = s;
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBacklogMilestoneBody {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBacklogMilestoneBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

fn apply_milestone_update(m: &mut BacklogMilestone, body: &UpdateBacklogMilestoneBody) {
    if let Some(ref t) = body.title {
        let t = t.trim();
        if !t.is_empty() {
            m.title = t.to_string();
        }
    }
    if let Some(ref d) = body.description {
        m.description.clone_from(d);
    }
}

/// GET /api/projects/:id/backlog/decisions
pub async fn backlog_list_decisions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let rows: Vec<serde_json::Value> = snap
        .decisions
        .iter()
        .filter_map(|d| serde_json::to_value(d).ok())
        .collect();
    Json(rows).into_response()
}

/// GET /api/projects/:id/backlog/decisions/:decision_id
pub async fn backlog_get_decision(
    State(state): State<Arc<AppState>>,
    Path((id, decision_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state.backlog_store.get_decision(&pid, &decision_id) {
        Some(d) => match serde_json::to_value(&d) {
            Ok(v) => (StatusCode::OK, Json(v)).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Serialize failed"})),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Not found"})),
        )
            .into_response(),
    }
}

/// POST /api/projects/:id/backlog/decisions
pub async fn backlog_create_decision(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<CreateBacklogDecisionBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state.backlog_store.create_decision_with_title(
        &pid,
        body.title.trim(),
        &state.kernel.project_store,
    ) {
        Ok(d) => match serde_json::to_value(&d) {
            Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Serialize failed"})),
            )
                .into_response(),
        },
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// PUT /api/projects/:id/backlog/decisions/:decision_id
pub async fn backlog_update_decision(
    State(state): State<Arc<AppState>>,
    Path((id, decision_id)): Path<(String, String)>,
    Json(body): Json<UpdateBacklogDecisionBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let mut d = match state.backlog_store.get_decision(&pid, &decision_id) {
        Some(x) => x,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Not found"})),
            )
                .into_response();
        }
    };
    apply_decision_update(&mut d, &body);
    d.file_path = None;
    match state
        .backlog_store
        .update_decision(&pid, &d, &state.kernel.project_store)
    {
        Ok(()) => {
            let fresh = state
                .backlog_store
                .get_decision(&pid, &decision_id)
                .expect("decision after update");
            match serde_json::to_value(&fresh) {
                Ok(v) => (StatusCode::OK, Json(v)).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Serialize failed"})),
                )
                    .into_response(),
            }
        }
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// DELETE /api/projects/:id/backlog/decisions/:decision_id
pub async fn backlog_delete_decision(
    State(state): State<Arc<AppState>>,
    Path((id, decision_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state
        .backlog_store
        .delete_decision(&pid, &decision_id, &state.kernel.project_store)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// GET /api/projects/:id/backlog/milestones/archived
pub async fn backlog_list_archived_milestones(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let rows: Vec<serde_json::Value> = snap
        .archived_milestones
        .iter()
        .filter_map(|m| serde_json::to_value(m).ok())
        .collect();
    Json(rows).into_response()
}

/// GET /api/projects/:id/backlog/milestones
pub async fn backlog_list_milestones(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let rows: Vec<serde_json::Value> = snap
        .milestones
        .iter()
        .map(|m| milestone_with_stats_json(&snap, m))
        .collect();
    Json(rows).into_response()
}

/// POST /api/projects/:id/backlog/milestones
pub async fn backlog_create_milestone(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<CreateBacklogMilestoneBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let desc = body.description.as_deref().unwrap_or("");
    match state.backlog_store.create_milestone(
        &pid,
        body.title.trim(),
        desc,
        &state.kernel.project_store,
    ) {
        Ok(m) => {
            let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Backlog not loaded"})),
                )
                    .into_response();
            };
            let v = milestone_with_stats_json(&snap, &m);
            (StatusCode::CREATED, Json(v)).into_response()
        }
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// PUT /api/projects/:id/backlog/milestones/:milestone_id
pub async fn backlog_update_milestone(
    State(state): State<Arc<AppState>>,
    Path((id, milestone_id)): Path<(String, String)>,
    Json(body): Json<UpdateBacklogMilestoneBody>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let mut m = match state.backlog_store.get_milestone(&pid, &milestone_id) {
        Some(x) => x,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Not found"})),
            )
                .into_response();
        }
    };
    apply_milestone_update(&mut m, &body);
    m.file_path = None;
    match state
        .backlog_store
        .update_milestone(&pid, &m, &state.kernel.project_store)
    {
        Ok(()) => {
            let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Backlog not loaded"})),
                )
                    .into_response();
            };
            let fresh = state
                .backlog_store
                .get_milestone(&pid, &milestone_id)
                .expect("milestone after update");
            let v = milestone_with_stats_json(&snap, &fresh);
            (StatusCode::OK, Json(v)).into_response()
        }
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// DELETE /api/projects/:id/backlog/milestones/:milestone_id
pub async fn backlog_delete_milestone(
    State(state): State<Arc<AppState>>,
    Path((id, milestone_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state
        .backlog_store
        .delete_milestone(&pid, &milestone_id, &state.kernel.project_store)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// POST /api/projects/:id/backlog/milestones/:milestone_id/archive
pub async fn backlog_archive_milestone(
    State(state): State<Arc<AppState>>,
    Path((id, milestone_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    match state
        .backlog_store
        .archive_milestone(&pid, &milestone_id, &state.kernel.project_store)
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_backlog_err(e).into_response(),
    }
}

/// GET /api/projects/:id/backlog/completed
pub async fn backlog_list_completed(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let rows: Vec<serde_json::Value> = snap
        .completed
        .iter()
        .filter_map(|t| serde_json::to_value(t).ok())
        .collect();
    Json(rows).into_response()
}

// --- Search & statistics -------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct BacklogSearchQuery {
    #[serde(default)]
    pub q: Option<String>,
    /// `type=task` once or repeated (serde_urlencoded uses a single string for one value).
    #[serde(
        default,
        rename = "type",
        deserialize_with = "deserialize_one_or_many_strings"
    )]
    pub kinds: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_one_or_many_strings")]
    pub status: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_one_or_many_strings")]
    pub priority: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_one_or_many_strings")]
    pub label: Vec<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogStatisticsResponse {
    pub total: u64,
    pub status_counts: BTreeMap<String, u64>,
    pub priority_counts: BTreeMap<String, u64>,
}

/// Aggregated backlog counts for the project overview dashboard.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBacklogOverviewResponse {
    pub task_statistics: BacklogStatisticsResponse,
    pub active_task_count: u64,
    pub completed_task_count: u64,
    pub document_count: u64,
    pub decision_count: u64,
    pub decisions_by_status: BTreeMap<String, u64>,
    pub milestone_count: u64,
}

fn backlog_overview_build(snap: &BacklogSnapshot) -> ProjectBacklogOverviewResponse {
    let mut decisions_by_status: BTreeMap<String, u64> = BTreeMap::new();
    for d in &snap.decisions {
        let k = decision_status_token(d.status).to_string();
        *decisions_by_status.entry(k).or_insert(0) += 1;
    }
    ProjectBacklogOverviewResponse {
        task_statistics: backlog_statistics_build(snap),
        active_task_count: snap.tasks.len() as u64,
        completed_task_count: snap.completed.len() as u64,
        document_count: snap.documents.len() as u64,
        decision_count: snap.decisions.len() as u64,
        decisions_by_status,
        milestone_count: snap.milestones.len() as u64,
    }
}

fn decision_status_token(s: DecisionStatus) -> &'static str {
    match s {
        DecisionStatus::Proposed => "proposed",
        DecisionStatus::Accepted => "accepted",
        DecisionStatus::Rejected => "rejected",
        DecisionStatus::Superseded => "superseded",
    }
}

fn search_needle(q: &Option<String>) -> Option<String> {
    q.as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
}

fn wants_kind(kinds: &[String], want: &str) -> bool {
    if kinds.is_empty() {
        return true;
    }
    kinds
        .iter()
        .any(|t| t.trim().eq_ignore_ascii_case(want))
}

fn matches_status_list(value: &str, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    let v = value.trim();
    filters
        .iter()
        .any(|f| v.eq_ignore_ascii_case(f.trim()))
}

fn task_matches_search_filters(t: &BacklogTask, q: &BacklogSearchQuery) -> bool {
    if !matches_status_list(&t.status, &q.status) {
        return false;
    }
    if !q.priority.is_empty() {
        let want_any_none = q
            .priority
            .iter()
            .any(|p| p.trim().eq_ignore_ascii_case("none"));
        match t.priority {
            Some(pr) => {
                let got = match pr {
                    TaskPriority::High => "high",
                    TaskPriority::Medium => "medium",
                    TaskPriority::Low => "low",
                };
                if !matches_status_list(got, &q.priority) {
                    return false;
                }
            }
            None => {
                if !want_any_none {
                    return false;
                }
            }
        }
    }
    if !q.label.is_empty()
        && !q.label.iter().any(|want| {
            let w = want.trim().to_lowercase();
            t.labels
                .iter()
                .any(|l| l.to_lowercase().contains(&w) || l.to_lowercase() == w)
        })
    {
        return false;
    }
    true
}

fn decision_matches_search_filters(d: &BacklogDecision, q: &BacklogSearchQuery) -> bool {
    if q.priority.is_empty() && q.label.is_empty() {
        return matches_status_list(decision_status_token(d.status), &q.status);
    }
    if !q.status.is_empty()
        && !matches_status_list(decision_status_token(d.status), &q.status)
    {
        return false;
    }
    !(!q.priority.is_empty() || !q.label.is_empty())
}

fn task_search_body(t: &BacklogTask) -> String {
    let mut s = String::new();
    if let Some(ref d) = t.description {
        s.push_str(d);
        s.push('\n');
    }
    s.push_str(&t.raw_content);
    s.push('\n');
    for ac in &t.acceptance_criteria {
        s.push_str(&ac.text);
        s.push('\n');
    }
    s
}

fn score_text_match(title: &str, body: &str, needle: Option<&str>) -> Option<f64> {
    let Some(n) = needle else {
        return Some(1.0);
    };
    if n.is_empty() {
        return Some(1.0);
    }
    let tl = title.to_lowercase();
    let bl = body.to_lowercase();
    let mut best = 0.0_f64;
    if tl == *n {
        best = best.max(1.0);
    } else if tl.contains(n) {
        best = best.max(0.8);
    }
    if bl.contains(n) {
        best = best.max(0.5);
    }
    (best > 0.0).then_some(best)
}

fn score_task(t: &BacklogTask, needle: Option<&str>) -> Option<f64> {
    let body = task_search_body(t);
    score_text_match(&t.title, &body, needle)
}

fn score_document(d: &BacklogDocument, needle: Option<&str>) -> Option<f64> {
    score_text_match(&d.title, &d.raw_content, needle)
}

fn score_decision(d: &BacklogDecision, needle: Option<&str>) -> Option<f64> {
    let Some(n) = needle else {
        return Some(1.0);
    };
    if n.is_empty() {
        return Some(1.0);
    }
    let title = d.title.to_lowercase();
    let body = format!(
        "{}\n{}\n{}",
        d.context, d.decision, d.consequences
    )
    .to_lowercase();
    let mut best = 0.0_f64;
    if title == *n {
        best = best.max(1.0);
    } else if title.contains(n) {
        best = best.max(0.8);
    }
    if body.contains(n) {
        best = best.max(0.5);
    }
    (best > 0.0).then_some(best)
}

fn backlog_search_run(snap: &BacklogSnapshot, q: &BacklogSearchQuery) -> Vec<BacklogSearchResult> {
    let needle = search_needle(&q.q);
    let needle_ref = needle.as_deref();
    let limit = q.limit.unwrap_or(50).clamp(1, 200) as usize;

    let mut scored: Vec<(f64, String, BacklogSearchResult)> = Vec::new();

    if wants_kind(&q.kinds, "task") {
        for t in snap.tasks.iter().chain(&snap.completed) {
            if !task_matches_search_filters(t, q) {
                continue;
            }
            let Some(score) = score_task(t, needle_ref) else {
                continue;
            };
            scored.push((
                score,
                t.id.clone(),
                BacklogSearchResult::Task {
                    score: Some(score),
                    task: Box::new(t.clone()),
                },
            ));
        }
    }

    if wants_kind(&q.kinds, "document") {
        for d in &snap.documents {
            if !q.status.is_empty() || !q.priority.is_empty() || !q.label.is_empty() {
                continue;
            }
            let Some(score) = score_document(d, needle_ref) else {
                continue;
            };
            scored.push((
                score,
                d.id.clone(),
                BacklogSearchResult::Document {
                    score: Some(score),
                    document: Box::new(d.clone()),
                },
            ));
        }
    }

    if wants_kind(&q.kinds, "decision") {
        for d in &snap.decisions {
            if !decision_matches_search_filters(d, q) {
                continue;
            }
            let Some(score) = score_decision(d, needle_ref) else {
                continue;
            };
            scored.push((
                score,
                d.id.clone(),
                BacklogSearchResult::Decision {
                    score: Some(score),
                    decision: Box::new(d.clone()),
                },
            ));
        }
    }

    scored.sort_by(|a, b| {
        b.0
            .partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, r)| r)
        .collect()
}

fn backlog_statistics_build(snap: &BacklogSnapshot) -> BacklogStatisticsResponse {
    let mut status_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut priority_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut total = 0u64;
    for t in snap.tasks.iter().chain(&snap.completed) {
        total += 1;
        *status_counts.entry(t.status.clone()).or_insert(0) += 1;
        let pk = match t.priority {
            Some(TaskPriority::High) => "high",
            Some(TaskPriority::Medium) => "medium",
            Some(TaskPriority::Low) => "low",
            None => "none",
        };
        *priority_counts.entry(pk.to_string()).or_insert(0) += 1;
    }
    BacklogStatisticsResponse {
        total,
        status_counts,
        priority_counts,
    }
}

/// GET /api/projects/:id/backlog/search
pub async fn backlog_search(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<BacklogSearchQuery>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let results = backlog_search_run(&snap, &query);
    match serde_json::to_value(&results) {
        Ok(v) => Json(v).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Serialize failed"})),
        )
            .into_response(),
    }
}

/// GET /api/projects/:id/backlog/overview — task/doc/decision/milestone counts for project home.
pub async fn backlog_overview(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let overview = backlog_overview_build(&snap);
    match serde_json::to_value(&overview) {
        Ok(v) => Json(v).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Serialize failed"})),
        )
            .into_response(),
    }
}

/// GET /api/projects/:id/backlog/statistics
pub async fn backlog_statistics(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let pid = match parse_project_id_param(&id) {
        Ok(p) => p,
        Err(tup) => return tup.into_response(),
    };
    if let Err(tup) = ensure_project(&state, pid) {
        return tup.into_response();
    }
    let Some(snap) = state.backlog_store.get_snapshot(&pid) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Backlog not loaded"})),
        )
            .into_response();
    };
    let stats = backlog_statistics_build(&snap);
    match serde_json::to_value(&stats) {
        Ok(v) => Json(v).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Serialize failed"})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn score_task_title_exact_beats_body() {
        let t = BacklogTask {
            id: "TASK-1".into(),
            title: "Alpha".into(),
            status: "Open".into(),
            created_date: "2026-01-01".into(),
            description: Some("alpha body".into()),
            ..Default::default()
        };
        assert_eq!(score_task(&t, Some("alpha")), Some(1.0));
    }

    #[test]
    fn score_task_body_only() {
        let t = BacklogTask {
            id: "TASK-1".into(),
            title: "Z".into(),
            status: "Open".into(),
            created_date: "2026-01-01".into(),
            description: Some("findme here".into()),
            ..Default::default()
        };
        assert_eq!(score_task(&t, Some("findme")), Some(0.5));
    }

    #[test]
    fn score_ac_text_matches() {
        let t = BacklogTask {
            id: "TASK-1".into(),
            title: "Z".into(),
            status: "Open".into(),
            created_date: "2026-01-01".into(),
            acceptance_criteria: vec![AcceptanceCriterion {
                index: 0,
                text: "unique-ac-phrase".into(),
                checked: false,
            }],
            ..Default::default()
        };
        assert_eq!(score_task(&t, Some("unique-ac-phrase")), Some(0.5));
    }

    #[test]
    fn search_sorts_by_score_then_id() {
        let snap = BacklogSnapshot {
            tasks: vec![
                BacklogTask {
                    id: "TASK-B".into(),
                    title: "B body match".into(),
                    status: "Open".into(),
                    created_date: "2026-01-01".into(),
                    description: Some("qqq".into()),
                    ..Default::default()
                },
                BacklogTask {
                    id: "TASK-A".into(),
                    title: "qqq title".into(),
                    status: "Open".into(),
                    created_date: "2026-01-01".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let q = BacklogSearchQuery {
            q: Some("qqq".into()),
            limit: Some(10),
            ..Default::default()
        };
        let out = backlog_search_run(&snap, &q);
        assert_eq!(out.len(), 2);
        match &out[0] {
            BacklogSearchResult::Task { task, .. } => assert_eq!(task.id, "TASK-A"),
            _ => panic!("expected task"),
        }
    }

    #[test]
    fn search_query_urlencoded_single_type_and_priority() {
        let q: BacklogSearchQuery =
            serde_urlencoded::from_str("q=a&type=task&priority=high").unwrap();
        assert_eq!(q.kinds, vec!["task".to_string()]);
        assert_eq!(q.priority, vec!["high".to_string()]);
    }

    #[test]
    fn statistics_counts_status_and_priority() {
        let snap = BacklogSnapshot {
            tasks: vec![
                BacklogTask {
                    id: "T1".into(),
                    title: "a".into(),
                    status: "Open".into(),
                    created_date: "2026-01-01".into(),
                    priority: Some(TaskPriority::High),
                    ..Default::default()
                },
                BacklogTask {
                    id: "T2".into(),
                    title: "b".into(),
                    status: "Done".into(),
                    created_date: "2026-01-01".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let s = backlog_statistics_build(&snap);
        assert_eq!(s.total, 2);
        assert_eq!(s.status_counts.get("Open"), Some(&1));
        assert_eq!(s.status_counts.get("Done"), Some(&1));
        assert_eq!(s.priority_counts.get("high"), Some(&1));
        assert_eq!(s.priority_counts.get("none"), Some(&1));
    }
}
