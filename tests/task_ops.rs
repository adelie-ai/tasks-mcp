use serde_json::json;
use tempfile::TempDir;

use tasks_mcp::operations::task_ops::{
    AddExternalRefInput, AppendTaskNoteInput, CreateTaskInput, ListTasksInput, RelationshipInput,
    RepairStrategy, RepairTaskFrontmatterInput, SearchTasksInput, SetStatusInput, TaskLocator,
    UpdateTaskInput, add_deliverable, add_external_ref, append_task_note, create_task, get_task,
    list_tasks, remove_deliverable, repair_task_frontmatter, search_tasks, set_status, update_task,
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
            assignee: None,
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
            assignee: None,
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
            assignee: None,
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

fn make_task(list: &str) -> CreateTaskInput {
    CreateTaskInput {
        list: list.to_string(),
        task_type: tasks_mcp::model::TaskType::Deliverable,
        title: "Test task".to_string(),
        status: Some(tasks_mcp::model::TaskStatus::Todo),
        epic_id: None,
        deliverable_ids: None,
        tags: None,
        priority: None,
        due: None,
        links: None,
        assignee: None,
        body: Some("Initial body.".to_string()),
    }
}

#[tokio::test]
async fn append_task_note_adds_to_body_without_touching_frontmatter() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let id = created["id"].as_str().expect("id").to_string();

    append_task_note(
        &storage,
        AppendTaskNoteInput {
            locator: TaskLocator { id: Some(id.clone()), path: None },
            note: "Ticket confirmed".to_string(),
            section: None,
            timestamp: Some(false),
        },
    )
    .await
    .expect("append note");

    let fetched = get_task(&storage, TaskLocator { id: Some(id), path: None })
        .await
        .expect("get task");

    let body = fetched["body"].as_str().expect("body");
    assert!(body.contains("Initial body."), "original body preserved");
    assert!(body.contains("Ticket confirmed"), "note appended");

    // Frontmatter must still round-trip cleanly.
    assert_eq!(fetched["frontmatter"]["status"].as_str(), Some("todo"));
}

#[tokio::test]
async fn append_task_note_creates_section_if_absent() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let id = created["id"].as_str().expect("id").to_string();

    append_task_note(
        &storage,
        AppendTaskNoteInput {
            locator: TaskLocator { id: Some(id.clone()), path: None },
            note: "Access granted".to_string(),
            section: Some("Notes".to_string()),
            timestamp: Some(false),
        },
    )
    .await
    .expect("append note under section");

    let fetched = get_task(&storage, TaskLocator { id: Some(id), path: None })
        .await
        .expect("get task");

    let body = fetched["body"].as_str().expect("body");
    assert!(body.contains("## Notes"), "heading created");
    assert!(body.contains("Access granted"), "note under heading");
}

#[tokio::test]
async fn append_task_note_inserts_under_existing_section() {
    let (_dir, storage) = test_storage();

    let mut input = make_task("ops");
    input.body = Some("## Notes\nFirst note.\n\n## Other".to_string());
    let created = create_task(&storage, input).await.expect("create");
    let id = created["id"].as_str().expect("id").to_string();

    append_task_note(
        &storage,
        AppendTaskNoteInput {
            locator: TaskLocator { id: Some(id.clone()), path: None },
            note: "Second note.".to_string(),
            section: Some("Notes".to_string()),
            timestamp: Some(false),
        },
    )
    .await
    .expect("append under existing section");

    let fetched = get_task(&storage, TaskLocator { id: Some(id), path: None })
        .await
        .expect("get task");

    let body = fetched["body"].as_str().expect("body");
    // Both notes appear and ## Other still follows Notes.
    assert!(body.contains("First note."));
    assert!(body.contains("Second note."));
    assert!(body.contains("## Other"));
    let notes_pos = body.find("## Notes").unwrap();
    let other_pos = body.find("## Other").unwrap();
    assert!(notes_pos < other_pos, "## Other stays after ## Notes");
}

#[tokio::test]
async fn add_external_ref_stores_and_round_trips() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let id = created["id"].as_str().expect("id").to_string();

    add_external_ref(
        &storage,
        AddExternalRefInput {
            locator: TaskLocator { id: Some(id.clone()), path: None },
            system: "jira".to_string(),
            reference: "PROJ-42".to_string(),
            url: Some("https://jira.example.com/browse/PROJ-42".to_string()),
        },
    )
    .await
    .expect("add external ref");

    let fetched = get_task(&storage, TaskLocator { id: Some(id), path: None })
        .await
        .expect("get task");

    let refs = fetched["frontmatter"]["external_refs"]
        .as_array()
        .expect("external_refs array");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["system"].as_str(), Some("jira"));
    assert_eq!(refs[0]["ref"].as_str(), Some("PROJ-42"));
    assert_eq!(
        refs[0]["url"].as_str(),
        Some("https://jira.example.com/browse/PROJ-42")
    );
}

#[tokio::test]
async fn add_external_ref_deduplicates() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let id = created["id"].as_str().expect("id").to_string();

    for _ in 0..3 {
        add_external_ref(
            &storage,
            AddExternalRefInput {
                locator: TaskLocator { id: Some(id.clone()), path: None },
                system: "github".to_string(),
                reference: "org/repo#7".to_string(),
                url: None,
            },
        )
        .await
        .expect("add external ref");
    }

    let fetched = get_task(&storage, TaskLocator { id: Some(id), path: None })
        .await
        .expect("get task");

    let refs = fetched["frontmatter"]["external_refs"]
        .as_array()
        .expect("external_refs array");
    assert_eq!(refs.len(), 1, "duplicates collapsed to one entry");
}

#[tokio::test]
async fn add_external_ref_markdown_special_chars_do_not_corrupt() {
    // Regression: bold Markdown in a ref value must not break YAML parsing.
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let id = created["id"].as_str().expect("id").to_string();

    add_external_ref(
        &storage,
        AddExternalRefInput {
            locator: TaskLocator { id: Some(id.clone()), path: None },
            system: "internal".to_string(),
            reference: "**BOLD-REF**".to_string(),
            url: None,
        },
    )
    .await
    .expect("add ref with special chars");

    // Must be readable again without error.
    let fetched = get_task(&storage, TaskLocator { id: Some(id), path: None })
        .await
        .expect("task readable after bold ref");

    let refs = fetched["frontmatter"]["external_refs"]
        .as_array()
        .expect("external_refs");
    assert_eq!(refs[0]["ref"].as_str(), Some("**BOLD-REF**"));
}

#[tokio::test]
async fn update_task_body_append_and_prepend() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let id = created["id"].as_str().expect("id").to_string();

    update_task(
        &storage,
        UpdateTaskInput {
            locator: TaskLocator { id: Some(id.clone()), path: None },
            patch: json!({"body_append": "appended line"}),
        },
    )
    .await
    .expect("body_append");

    update_task(
        &storage,
        UpdateTaskInput {
            locator: TaskLocator { id: Some(id.clone()), path: None },
            patch: json!({"body_prepend": "prepended line"}),
        },
    )
    .await
    .expect("body_prepend");

    let fetched = get_task(&storage, TaskLocator { id: Some(id), path: None })
        .await
        .expect("get task");

    let body = fetched["body"].as_str().expect("body");
    assert!(body.starts_with("prepended line"), "prepend is at start");
    assert!(body.contains("Initial body."), "original body preserved");
    assert!(body.ends_with("appended line"), "append is at end");
}

#[tokio::test]
async fn repair_salvage_makes_corrupt_task_readable() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let path = created["path"].as_str().expect("path").to_string();

    // Corrupt the frontmatter by injecting raw Markdown bold that breaks YAML.
    let original = tokio::fs::read_to_string(&path).await.expect("read");
    let corrupted = original.replacen("title:", "title: **broken** yaml * here:", 1);
    tokio::fs::write(&path, &corrupted).await.expect("write corrupted");

    let result = repair_task_frontmatter(
        &storage,
        RepairTaskFrontmatterInput {
            locator: TaskLocator { id: None, path: Some(path.clone()) },
            strategy: RepairStrategy::Salvage,
            dry_run: Some(false),
        },
    )
    .await
    .expect("repair should succeed");

    assert_eq!(result["repaired"].as_bool(), Some(true));

    // File must now be parseable via get_task.
    get_task(&storage, TaskLocator { id: None, path: Some(path.clone()) })
        .await
        .expect("repaired task is readable");

    // The corrupt YAML should be preserved in the body under the recovery heading.
    let repaired_content = tokio::fs::read_to_string(&path).await.expect("read repaired");
    assert!(repaired_content.contains("## Recovered Frontmatter"));
}

#[tokio::test]
async fn repair_dry_run_does_not_modify_file() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let path = created["path"].as_str().expect("path").to_string();

    let original = tokio::fs::read_to_string(&path).await.expect("read");
    let corrupted = original.replacen("title:", "title: **broken**:", 1);
    tokio::fs::write(&path, &corrupted).await.expect("write corrupted");

    let result = repair_task_frontmatter(
        &storage,
        RepairTaskFrontmatterInput {
            locator: TaskLocator { id: None, path: Some(path.clone()) },
            strategy: RepairStrategy::Reset,
            dry_run: Some(true),
        },
    )
    .await
    .expect("dry_run repair");

    assert_eq!(result["dry_run"].as_bool(), Some(true));
    assert!(result["preview"].as_str().is_some(), "preview returned");

    // File on disk must be unchanged.
    let on_disk = tokio::fs::read_to_string(&path).await.expect("read after dry run");
    assert_eq!(on_disk, corrupted, "file not modified during dry_run");
}

#[tokio::test]
async fn repair_already_valid_returns_no_op() {
    let (_dir, storage) = test_storage();

    let created = create_task(&storage, make_task("ops")).await.expect("create");
    let path = created["path"].as_str().expect("path").to_string();

    let result = repair_task_frontmatter(
        &storage,
        RepairTaskFrontmatterInput {
            locator: TaskLocator { id: None, path: Some(path) },
            strategy: RepairStrategy::Salvage,
            dry_run: None,
        },
    )
    .await
    .expect("repair on valid file");

    assert_eq!(result["repaired"].as_bool(), Some(false));
}

#[tokio::test]
async fn set_status_validating_and_list_filter() {
    let (_dir, storage) = test_storage();
    storage
        .create_list("project-alpha")
        .await
        .expect("create list");

    // Create two tasks; set one to validating.
    let task_a = create_task(
        &storage,
        CreateTaskInput {
            list: "project-alpha".to_string(),
            task_type: tasks_mcp::model::TaskType::Deliverable,
            title: "Awaiting validation".to_string(),
            status: Some(tasks_mcp::model::TaskStatus::Doing),
            epic_id: None,
            deliverable_ids: None,
            tags: None,
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: None,
        },
    )
    .await
    .expect("create task a");

    let _task_b = create_task(
        &storage,
        CreateTaskInput {
            list: "project-alpha".to_string(),
            task_type: tasks_mcp::model::TaskType::Deliverable,
            title: "Still in progress".to_string(),
            status: Some(tasks_mcp::model::TaskStatus::Doing),
            epic_id: None,
            deliverable_ids: None,
            tags: None,
            priority: None,
            due: None,
            links: None,
            assignee: None,
            body: None,
        },
    )
    .await
    .expect("create task b");

    let id_a = task_a["id"].as_str().expect("id").to_string();

    // set_status returns the new status.
    let result = set_status(
        &storage,
        SetStatusInput {
            locator: TaskLocator {
                id: Some(id_a.clone()),
                path: None,
            },
            status: tasks_mcp::model::TaskStatus::Validating,
        },
    )
    .await
    .expect("set status");
    assert_eq!(result["status"].as_str(), Some("validating"));

    // get_task reflects the persisted status.
    let fetched = get_task(
        &storage,
        TaskLocator {
            id: Some(id_a),
            path: None,
        },
    )
    .await
    .expect("get task");
    assert_eq!(
        fetched["frontmatter"]["status"].as_str(),
        Some("validating")
    );

    // list_tasks filtered by validating returns exactly one task.
    let validating_tasks = list_tasks(
        &storage,
        ListTasksInput {
            list: None,
            lists: None,
            task_type: None,
            status: Some(tasks_mcp::model::TaskStatus::Validating),
            tag: None,
            epic_id: None,
            assignee: None,
        },
    )
    .await
    .expect("list validating");
    assert_eq!(validating_tasks.as_array().map(Vec::len), Some(1));
    assert_eq!(
        validating_tasks[0]["status"].as_str(),
        Some("validating")
    );
}
