use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;

/// Información plana de un archivo (para CSV).
#[derive(Debug, Clone, Serialize)]
pub struct FileRecord {
    pub name: String,
    pub extension: String,
    pub size_mb: f64,
    pub created: String,
    pub modified: String,
    pub directory: String,
    pub full_path: String,
}

/// Nodo del árbol jerárquico (para JSON y Markdown).
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Node {
    Directory {
        name: String,
        path: PathBuf,
        children: Vec<Node>,
    },
    File {
        name: String,
        path: PathBuf,
        size_mb: f64,
        extension: String,
        created: Option<DateTime<Utc>>,
        modified: Option<DateTime<Utc>>,
    },
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::Directory { name, .. } | Node::File { name, .. } => name,
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, Node::Directory { .. })
    }
}