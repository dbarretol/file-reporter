use crate::models::{FileRecord, Node};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Convierte SystemTime a DateTime<Utc> de forma segura.
fn to_datetime(t: SystemTime) -> Option<DateTime<Utc>> {
    Some(DateTime::<Utc>::from(t))
}

/// Comprueba si la extensión del archivo coincide con el filtro.
fn matches_extension(path: &Path, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => filters.iter().any(|f| f.eq_ignore_ascii_case(ext)),
        None => false,
    }
}

/// Construye recursivamente el árbol jerárquico.
/// Los errores de lectura (permisos, etc.) se ignoran silenciosamente
/// por archivo/carpeta para que el escaneo sea robusto.
pub fn build_tree(path: &Path, filters: &[String]) -> Result<Node> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("No se pudo leer metadatos de: {}", path.display()))?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    if metadata.is_dir() {
        // Recolectamos rutas hijas
        let entries: Vec<PathBuf> = match fs::read_dir(path) {
            Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
            Err(_) => Vec::new(), // sin permisos -> directorio vacío
        };

        // Procesamos hijos en paralelo
        let mut children: Vec<Node> = entries
            .par_iter()
            .filter_map(|p| build_tree(p, filters).ok())
            .filter(|n| match n {
                // Conservamos archivos que pasen el filtro
                Node::File { extension, .. } => {
                    filters.is_empty()
                        || filters.iter().any(|f| f.eq_ignore_ascii_case(extension))
                }
                // Conservamos directorios solo si tienen contenido tras filtrar
                Node::Directory { children, .. } => !children.is_empty() || filters.is_empty(),
            })
            .collect();

        // Orden alfabético: directorios primero, luego archivos
        children.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
        });

        Ok(Node::Directory {
            name,
            path: path.to_path_buf(),
            children,
        })
    } else {
        let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        Ok(Node::File {
            name: name.clone(),
            path: path.to_path_buf(),
            size_mb,
            extension,
            created: metadata.created().ok().and_then(to_datetime),
            modified: metadata.modified().ok().and_then(to_datetime),
        })
    }
}

/// Aplana el árbol a una lista de FileRecord (para CSV),
/// aplicando el filtro de extensiones.
pub fn flatten_to_records(node: &Node, filters: &[String], out: &mut Vec<FileRecord>) {
    match node {
        Node::Directory { children, .. } => {
            for c in children {
                flatten_to_records(c, filters, out);
            }
        }
        Node::File {
            path,
            size_mb,
            extension,
            created,
            modified,
            ..
        } => {
            if !matches_extension(path, filters) {
                return;
            }
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir = path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            out.push(FileRecord {
                name: stem,
                extension: extension.clone(),
                size_mb: (size_mb * 1000.0).round() / 1000.0, // 3 decimales
                // 🆕 Formato DD/MM/AAAA (solo fecha, sin hora)
                created: created
                    .map(|d| d.format("%d/%m/%Y").to_string())
                    .unwrap_or_default(),
                modified: modified
                    .map(|d| d.format("%d/%m/%Y").to_string())
                    .unwrap_or_default(),
                directory: dir,
                full_path: path.display().to_string(),
            });
        }
    }
}