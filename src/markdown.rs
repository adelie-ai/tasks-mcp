#![deny(warnings)]

use chrono::{DateTime, NaiveDate};

use crate::error::{Result, TaskMcpError};
use crate::model::{TaskDocument, TaskFrontmatter, TaskType};

const FRONTMATTER_DELIMITER: &str = "---";

pub fn parse_task_markdown(path: String, content: &str) -> Result<TaskDocument> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with(FRONTMATTER_DELIMITER) {
        return Err(TaskMcpError::InvalidTaskDocument(
            "missing YAML frontmatter".to_string(),
        ));
    }

    let mut lines = trimmed.lines();
    let first = lines.next().unwrap_or_default();
    if first.trim() != FRONTMATTER_DELIMITER {
        return Err(TaskMcpError::InvalidTaskDocument(
            "invalid frontmatter opening delimiter".to_string(),
        ));
    }

    let mut yaml_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_yaml = true;

    for line in lines {
        if in_yaml && line.trim() == FRONTMATTER_DELIMITER {
            in_yaml = false;
            continue;
        }

        if in_yaml {
            yaml_lines.push(line);
        } else {
            body_lines.push(line);
        }
    }

    if in_yaml {
        return Err(TaskMcpError::InvalidTaskDocument(
            "missing frontmatter closing delimiter".to_string(),
        ));
    }

    let yaml_str = yaml_lines.join("\n");
    let frontmatter: TaskFrontmatter = serde_yaml::from_str(&yaml_str)?;
    validate_frontmatter(&frontmatter)?;

    Ok(TaskDocument {
        frontmatter,
        body: body_lines.join("\n").trim_start_matches('\n').to_string(),
        path,
    })
}

pub fn render_task_markdown(frontmatter: &TaskFrontmatter, body: &str) -> Result<String> {
    validate_frontmatter(frontmatter)?;
    let yaml = serde_yaml::to_string(frontmatter)?;
    let document = format!(
        "{d}\n{yaml}\n{d}\n\n{body}\n",
        d = FRONTMATTER_DELIMITER,
        yaml = yaml.trim_end(),
        body = body.trim_end()
    );
    Ok(document)
}

pub fn validate_frontmatter(frontmatter: &TaskFrontmatter) -> Result<()> {
    if frontmatter.id.trim().is_empty() {
        return Err(TaskMcpError::InvalidTaskDocument(
            "frontmatter.id is required".to_string(),
        ));
    }
    if frontmatter.title.trim().is_empty() {
        return Err(TaskMcpError::InvalidTaskDocument(
            "frontmatter.title is required".to_string(),
        ));
    }
    if frontmatter.list.trim().is_empty() {
        return Err(TaskMcpError::InvalidTaskDocument(
            "frontmatter.list is required".to_string(),
        ));
    }

    DateTime::parse_from_rfc3339(&frontmatter.created).map_err(|_| {
        TaskMcpError::InvalidTaskDocument(
            "frontmatter.created must be ISO-8601 datetime".to_string(),
        )
    })?;
    DateTime::parse_from_rfc3339(&frontmatter.updated).map_err(|_| {
        TaskMcpError::InvalidTaskDocument(
            "frontmatter.updated must be ISO-8601 datetime".to_string(),
        )
    })?;

    if let Some(due) = &frontmatter.due {
        NaiveDate::parse_from_str(due, "%Y-%m-%d").map_err(|_| {
            TaskMcpError::InvalidTaskDocument("frontmatter.due must be YYYY-MM-DD".to_string())
        })?;
    }

    match frontmatter.task_type {
        TaskType::Epic => {
            if frontmatter.epic_id.is_some() {
                return Err(TaskMcpError::InvalidTaskDocument(
                    "epic tasks cannot define epic_id".to_string(),
                ));
            }
        }
        TaskType::Deliverable => {
            if frontmatter.deliverable_ids.is_some() {
                return Err(TaskMcpError::InvalidTaskDocument(
                    "deliverable tasks cannot define deliverable_ids".to_string(),
                ));
            }
        }
    }

    Ok(())
}
