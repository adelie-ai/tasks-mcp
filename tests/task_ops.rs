use serde_json::json;
use tempfile::TempDir;

use tasks_mcp::operations::task_ops::{
    CreateTaskInput, ListTasksInput, RelationshipInput, SearchTasksInput, SetStatusInput,
    TaskLocator, UpdateTaskInput, add_deliverable, create_task, get_task, list_tasks,
    remove_deliverable, search_tasks, set_status, update_task,
};
use tasks_mcp::storage::Storage;

fn test_storage() -> (TempDir, Storage) {
    let dir = tempfile::tempdir().expect("tempdir");
    let storage = Storage::with_root(dir.path());
    (dir, storage)
}

#[tokio::test]
async fn create_and_get_task_roundtrip() {
    let (_dir, storage) = test_storage();
    storage
        .create_list("project-alpha")
        .await
        .expect("create list");

    let created = create_task(
        &storage,
        CreateTaskInput {
            list: "project-alpha".to_string(),
            task_type: tasks_mcp::model::TaskType::Deliverable,
            title: "Request environment access".to_string(),
            status: None,
            epic_id: None,
            deliverable_ids: None,
            tags: Some(vec!["access".to_string()]),
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: Some("## Checklist\n- [ ] Submit request".to_string()),
        },
    )
    .await
    .expect("create task");

    let id = created
        .get("id")
        .and_then(|value| value.as_str())
        .expect("id present")
        .to_string();

    let fetched = get_task(
        &storage,
        TaskLocator {
            id: Some(id),
            path: None,
        },
    )
    .await
    .expect("get task");

    assert_eq!(
        fetched
            .get("frontmatter")
            .and_then(|value| value.get("title"))
            .and_then(|value| value.as_str()),
        Some("Request environment access")
    );
}

#[tokio::test]
async fn list_search_and_relationships_work() {
    let (_dir, storage) = test_storage();

    let epic = create_task(
        &storage,
        CreateTaskInput {
            list: "project-alpha".to_string(),
            task_type: tasks_mcp::model::TaskType::Epic,
            title: "Project onboarding".to_string(),
            status: Some(tasks_mcp::model::TaskStatus::Doing),
            epic_id: None,
            deliverable_ids: None,
            tags: Some(vec!["setup".to_string()]),
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: Some("Summary: Onboard".to_string()),
        },
    )
    .await
    .expect("create epic");

    let deliverable = create_task(
        &storage,
        CreateTaskInput {
            list: "project-alpha".to_string(),
            task_type: tasks_mcp::model::TaskType::Deliverable,
            title: "Request environment access".to_string(),
            status: Some(tasks_mcp::model::TaskStatus::Todo),
            epic_id: None,
            deliverable_ids: None,
            tags: Some(vec!["access".to_string(), "environment".to_string()]),
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: Some("Need dev role".to_string()),
        },
    )
    .await
    .expect("create deliverable");

    let epic_id = epic
        .get("id")
        .and_then(|value| value.as_str())
        .expect("epic id")
        .to_string();
    let deliverable_id = deliverable
        .get("id")
        .and_then(|value| value.as_str())
        .expect("deliverable id")
        .to_string();

    add_deliverable(
        &storage,
        RelationshipInput {
            epic_id: epic_id.clone(),
            deliverable_id: deliverable_id.clone(),
        },
    )
    .await
    .expect("add relationship");

    let listed = list_tasks(
        &storage,
        ListTasksInput {
            list: None,
            lists: Some(vec!["project-alpha".to_string()]),
            task_type: Some(tasks_mcp::model::TaskType::Deliverable),
            status: None,
            tag: Some("access".to_string()),
            epic_id: Some(epic_id.clone()),
        },
    )
    .await
    .expect("list tasks");

    assert_eq!(listed.as_array().map(Vec::len), Some(1));

    let searched = search_tasks(
        &storage,
        SearchTasksInput {
            text: "dev role".to_string(),
            lists: Some(vec!["project-alpha".to_string()]),
        },
    )
    .await
    .expect("search tasks");
    assert_eq!(searched.as_array().map(Vec::len), Some(1));

    update_task(
        &storage,
        UpdateTaskInput {
            locator: TaskLocator {
                id: Some(deliverable_id.clone()),
                path: None,
            },
            patch: json!({"status": "doing"}),
        },
    )
    .await
    .expect("update task");

    remove_deliverable(
        &storage,
        RelationshipInput {
            epic_id,
            deliverable_id: deliverable_id.clone(),
        },
    )
    .await
    .expect("remove relationship");

    let fetched = get_task(
        &storage,
        TaskLocator {
            id: Some(deliverable_id),
            path: None,
        },
    )
    .await
    .expect("get deliverable");

    assert_eq!(
        fetched
            .get("frontmatter")
            .and_then(|value| value.get("epic_id"))
            .and_then(|value| value.as_str()),
        None
    );
}

#[tokio::test]
async fn list_tasks_supports_cross_list_and_subset_filtering() {
    let (_dir, storage) = test_storage();

    let _a = create_task(
        &storage,
        CreateTaskInput {
            list: "project-alpha".to_string(),
            task_type: tasks_mcp::model::TaskType::Deliverable,
            title: "Task A".to_string(),
            status: Some(tasks_mcp::model::TaskStatus::Todo),
            epic_id: None,
            deliverable_ids: None,
            tags: Some(vec!["shared".to_string()]),
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: Some("From project-alpha".to_string()),
        },
    )
    .await
    .expect("create project-alpha task");

    let _b = create_task(
        &storage,
        CreateTaskInput {
            list: "ops".to_string(),
            task_type: tasks_mcp::model::TaskType::Deliverable,
            title: "Task B".to_string(),
            status: Some(tasks_mcp::model::TaskStatus::Todo),
            epic_id: None,
            deliverable_ids: None,
            tags: Some(vec!["shared".to_string()]),
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: Some("From ops".to_string()),
        },
    )
    .await
    .expect("create ops task");

    let all_lists = list_tasks(
        &storage,
        ListTasksInput {
            list: None,
            lists: None,
            task_type: Some(tasks_mcp::model::TaskType::Deliverable),
            status: Some(tasks_mcp::model::TaskStatus::Todo),
            tag: Some("shared".to_string()),
            epic_id: None,
        },
    )
    .await
    .expect("list all lists");
    assert_eq!(all_lists.as_array().map(Vec::len), Some(2));

    let subset = list_tasks(
        &storage,
        ListTasksInput {
            list: None,
            lists: Some(vec!["ops".to_string()]),
            task_type: Some(tasks_mcp::model::TaskType::Deliverable),
            status: Some(tasks_mcp::model::TaskStatus::Todo),
            tag: Some("shared".to_string()),
            epic_id: None,
        },
    )
    .await
    .expect("list subset");

    let subset_arr = subset.as_array().expect("subset array");
    assert_eq!(subset_arr.len(), 1);
    assert_eq!(
        subset_arr[0].get("list").and_then(|value| value.as_str()),
        Some("ops")
    );

    let searched_subset = search_tasks(
        &storage,
        SearchTasksInput {
            text: "from".to_string(),
            lists: Some(vec!["ops".to_string()]),
        },
    )
    .await
    .expect("search subset");
    let searched_subset_arr = searched_subset.as_array().expect("subset search array");
    assert_eq!(searched_subset_arr.len(), 1);
    assert_eq!(
        searched_subset_arr[0]
            .get("list")
            .and_then(|value| value.as_str()),
        Some("ops")
    );

    let searched_global = search_tasks(
        &storage,
        SearchTasksInput {
            text: "from".to_string(),
            lists: None,
        },
    )
    .await
    .expect("search global");
    assert_eq!(searched_global.as_array().map(Vec::len), Some(2));
}

#[tokio::test]
async fn set_status_marks_task_done() {
    let (_dir, storage) = test_storage();

    let created = create_task(
        &storage,
        CreateTaskInput {
            list: "project-alpha".to_string(),
            task_type: tasks_mcp::model::TaskType::Deliverable,
            title: "Finish onboarding docs".to_string(),
            status: Some(tasks_mcp::model::TaskStatus::Todo),
            epic_id: None,
            deliverable_ids: None,
            tags: None,
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: Some("Initial draft".to_string()),
        },
    )
    .await
    .expect("create task");

    let id = created
        .get("id")
        .and_then(|value| value.as_str())
        .expect("id present")
        .to_string();

    let status_result = set_status(
        &storage,
        SetStatusInput {
            locator: TaskLocator {
                id: Some(id.clone()),
                path: None,
            },
            status: tasks_mcp::model::TaskStatus::Done,
        },
    )
    .await
    .expect("set status");
    assert_eq!(
        status_result.get("status").and_then(|value| value.as_str()),
        Some("done")
    );

    let fetched = get_task(
        &storage,
        TaskLocator {
            id: Some(id),
            path: None,
        },
    )
    .await
    .expect("get task");

    assert_eq!(
        fetched
            .get("frontmatter")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str()),
        Some("done")
    );
}
