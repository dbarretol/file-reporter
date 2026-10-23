use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;

/// Información plana de un archivo (para CSV).
#[derive(Debug, Clone, Serialize)]
pub struct FileRecord {
    pub name: String,
    pub extension: String,
    pub size_mb: f64,
    /// Tamaño exacto en bytes. `size_mb` redondea a 3 decimales y muestra
    /// "0.000" para archivos pequeños (<524 bytes); esta columna permite
    /// ordenar y sumar tamaños con precisión sin perder esa información.
    pub size_bytes: u64,
    pub created: String,
    pub modified: String,
    /// Fecha y hora en ISO-8601 (`2024-03-15T14:23:01Z`). `created`/`modified`
    /// usan `DD/MM/AAAA` por compatibilidad con Excel, pero ese formato no
    /// ordena como texto ni conserva la hora; estas columnas permiten
    /// ordenar cronológicamente y recuperar la hora exacta.
    pub created_iso: String,
    pub modified_iso: String,
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
        size_bytes: u64,
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
