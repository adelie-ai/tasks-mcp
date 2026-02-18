#![deny(warnings)]

use std::path::{Path, PathBuf};
use std::{env, path};

use chrono::Local;
use regex::Regex;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::{Result, TaskMcpError};
use crate::model::TaskType;

const DEFAULT_TASKS_ROOT: &str = "~/.local/share/desktop-assistant/tasks";

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new() -> Result<Self> {
        if let Ok(custom_root) = env::var("TASKS_MCP_ROOT")
            && !custom_root.trim().is_empty()
        {
            return Ok(Self {
                root: PathBuf::from(custom_root),
            });
        }

        let expanded = shellexpand::tilde(DEFAULT_TASKS_ROOT).to_string();
        Ok(Self {
            root: PathBuf::from(expanded),
        })
    }

    pub fn with_root<P>(root: P) -> Self
    where
        P: AsRef<path::Path>,
    {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn ensure_root(&self) -> Result<()> {
        fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    pub async fn list_lists(&self) -> Result<Vec<String>> {
        self.ensure_root().await?;
        let mut entries = fs::read_dir(&self.root).await?;
        let mut names = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let list_dir = entry.path();
            let epics = list_dir.join("epics");
            let deliverables = list_dir.join("deliverables");
            if epics.is_dir() && deliverables.is_dir() {
                names.push(name);
            }
        }

        names.sort();
        Ok(names)
    }

    pub async fn create_list(&self, name: &str) -> Result<()> {
        validate_list_name(name)?;
        fs::create_dir_all(self.list_dir(name).join("epics")).await?;
        fs::create_dir_all(self.list_dir(name).join("deliverables")).await?;
        Ok(())
    }

    pub fn list_dir(&self, list: &str) -> PathBuf {
        self.root.join(list)
    }

    pub fn type_dir(&self, list: &str, task_type: TaskType) -> PathBuf {
        self.list_dir(list).join(task_type.as_dir_name())
    }

    pub fn task_file_path(
        &self,
        list: &str,
        task_type: TaskType,
        id: &str,
        title: &str,
    ) -> PathBuf {
        let slug = slugify(title);
        self.type_dir(list, task_type)
            .join(format!("{id} - {slug}.md"))
    }

    pub async fn atomic_write(&self, path: &Path, content: &str) -> Result<()> {
        let parent = path.parent().ok_or_else(|| {
            TaskMcpError::InvalidArgument("target path has no parent directory".to_string())
        })?;
        fs::create_dir_all(parent).await?;

        let file_name = path
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| TaskMcpError::InvalidArgument("invalid target file name".to_string()))?;
        let temp_name = format!(".{file_name}.{}.tmp", Uuid::new_v4());
        let temp_path = parent.join(temp_name);

        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(content.as_bytes()).await?;
        file.sync_all().await?;
        drop(file);

        fs::rename(&temp_path, path).await?;
        Ok(())
    }

    pub async fn find_task_path_by_id(&self, id: &str) -> Result<PathBuf> {
        self.ensure_root().await?;
        let lists = self.list_lists().await?;
        for list in lists {
            for task_type in [TaskType::Epic, TaskType::Deliverable] {
                let dir = self.type_dir(&list, task_type);
                if !dir.is_dir() {
                    continue;
                }
                let mut entries = fs::read_dir(&dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let file_type = entry.file_type().await?;
                    if !file_type.is_file() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(id) && name.ends_with(".md") {
                        return Ok(entry.path());
                    }
                }
            }
        }

        Err(TaskMcpError::NotFound(format!("task id {id} not found")))
    }

    pub async fn all_task_paths(
        &self,
        list: Option<&str>,
        task_type: Option<TaskType>,
    ) -> Result<Vec<PathBuf>> {
        self.ensure_root().await?;
        let target_lists = if let Some(list_name) = list {
            vec![list_name.to_string()]
        } else {
            self.list_lists().await?
        };

        let target_types = if let Some(kind) = task_type {
            vec![kind]
        } else {
            vec![TaskType::Epic, TaskType::Deliverable]
        };

        let mut paths = Vec::new();
        for list_name in target_lists {
            for kind in &target_types {
                let dir = self.type_dir(&list_name, *kind);
                if !dir.is_dir() {
                    continue;
                }
                let mut entries = fs::read_dir(&dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    if entry.file_type().await?.is_file() {
                        let path = entry.path();
                        if path.extension().is_some_and(|ext| ext == "md") {
                            paths.push(path);
                        }
                    }
                }
            }
        }

        paths.sort();
        Ok(paths)
    }
}

pub fn validate_list_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(TaskMcpError::InvalidArgument(
            "list name cannot be empty".to_string(),
        ));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(TaskMcpError::InvalidArgument(
            "list name cannot contain path separators".to_string(),
        ));
    }
    Ok(())
}

pub fn generate_task_id() -> String {
    let now = Local::now();
    let suffix = Uuid::new_v4().to_string();
    let short_suffix = &suffix[..8];
    format!("tsk-{}-{short_suffix}", now.format("%Y%m%d-%H%M%S"))
}

pub fn now_iso8601() -> String {
    Local::now().to_rfc3339()
}

pub fn slugify(title: &str) -> String {
    let lowercase = title.to_lowercase();
    let cleaned = Regex::new(r"[^a-z0-9\s-]")
        .expect("regex compiles")
        .replace_all(&lowercase, "")
        .into_owned();
    let dashed = Regex::new(r"\s+")
        .expect("regex compiles")
        .replace_all(cleaned.trim(), "-")
        .into_owned();
    let collapsed = Regex::new(r"-+")
        .expect("regex compiles")
        .replace_all(&dashed, "-")
        .into_owned();

    if collapsed.is_empty() {
        "task".to_string()
    } else {
        collapsed
    }
}
