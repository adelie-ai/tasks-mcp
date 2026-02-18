#![deny(warnings)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    Epic,
    Deliverable,
}

impl TaskType {
    pub fn as_dir_name(self) -> &'static str {
        match self {
            TaskType::Epic => "epics",
            TaskType::Deliverable => "deliverables",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Todo,
    Doing,
    Blocked,
    Done,
    Canceled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFrontmatter {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub list: String,
    pub created: String,
    pub updated: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deliverable_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDocument {
    pub frontmatter: TaskFrontmatter,
    pub body: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub list: String,
    pub updated: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl From<&TaskDocument> for TaskSummary {
    fn from(value: &TaskDocument) -> Self {
        Self {
            id: value.frontmatter.id.clone(),
            title: value.frontmatter.title.clone(),
            task_type: value.frontmatter.task_type,
            status: value.frontmatter.status,
            list: value.frontmatter.list.clone(),
            updated: value.frontmatter.updated.clone(),
            path: value.path.clone(),
            epic_id: value.frontmatter.epic_id.clone(),
            tags: value.frontmatter.tags.clone(),
        }
    }
}
