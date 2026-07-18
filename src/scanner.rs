use crate::models::{FileRecord, Node};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::SystemTime;

/// Convierte SystemTime a DateTime<Utc> de forma segura.
fn to_datetime(t: SystemTime) -> Option<DateTime<Utc>> {
    Some(DateTime::<Utc>::from(t))
}

/// Construye recursivamente el árbol jerárquico.
/// Los errores de lectura (permisos, etc.) se ignoran silenciosamente
/// por archivo/carpeta para que el escaneo sea robusto.
///
/// Si se pasa un `counter`, se incrementa por cada archivo que pase el filtro
/// de extensión, a medida que se procesa (durante el I/O real), para reportar
/// progreso granular a la GUI.
///
/// `excluded` son rutas exactas (típicamente los reportes de la carpeta de
/// salida) que se omiten del escaneo para que una corrida no se incluya a
/// sí misma ni a la anterior.
///
/// Si se pasa `cancel` y está en `true`, se aborta el recorrido devolviendo
/// error en vez de seguir leyendo disco (cancelación pedida por el usuario).
///
/// Si se pasa `skipped`, se incrementa por cada carpeta o archivo que no se
/// pudo leer (permisos, entrada corrupta, etc. — no por symlinks omitidos
/// ni por cancelación, que son casos intencionales, no fallos), para que el
/// resultado final pueda avisar de que el reporte quedó incompleto.
pub fn build_tree(
    path: &Path,
    filters: &[String],
    counter: Option<&AtomicUsize>,
    excluded: &[PathBuf],
    cancel: Option<&AtomicBool>,
    skipped: Option<&AtomicUsize>,
) -> Result<Node> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        bail!("Escaneo cancelado");
    }

    // symlink_metadata (a diferencia de metadata) no sigue enlaces simbólicos
    // ni junctions de Windows, evitando recursión infinita en ciclos.
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            if let Some(s) = skipped {
                s.fetch_add(1, Ordering::Relaxed);
            }
            return Err(e)
                .with_context(|| format!("No se pudo leer metadatos de: {}", path.display()));
        }
    };

    if metadata.file_type().is_symlink() {
        bail!("Enlace simbólico omitido: {}", path.display());
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    if metadata.is_dir() {
        // Recolectamos rutas hijas, omitiendo las excluidas (reportes propios)
        let entries: Vec<PathBuf> = match fs::read_dir(path) {
            Ok(rd) => rd
                .filter_map(|e| match e {
                    Ok(entry) => Some(entry.path()),
                    Err(_) => {
                        if let Some(s) = skipped {
                            s.fetch_add(1, Ordering::Relaxed);
                        }
                        None
                    }
                })
                .filter(|p| !excluded.contains(p))
                .collect(),
            Err(_) => {
                // sin permisos -> no podemos listar el contenido; queda como
                // carpeta vacía en el árbol, pero contamos la pérdida.
                if let Some(s) = skipped {
                    s.fetch_add(1, Ordering::Relaxed);
                }
                Vec::new()
            }
        };

        // Procesamos hijos en paralelo
        let mut children: Vec<Node> = entries
            .par_iter()
            .filter_map(|p| build_tree(p, filters, counter, excluded, cancel, skipped).ok())
            .filter(|n| match n {
                // Conservamos archivos que pasen el filtro
                Node::File { extension, .. } => {
                    filters.is_empty() || filters.iter().any(|f| f.eq_ignore_ascii_case(extension))
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
        let size_bytes = metadata.len();
        let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Reportamos progreso aquí (durante el I/O real), no al aplanar el árbol.
        // Solo contamos archivos que pasarán el filtro de extensión, para que
        // el contador refleje lo que terminará en el reporte.
        let passes_filter =
            filters.is_empty() || filters.iter().any(|f| f.eq_ignore_ascii_case(&extension));
        if passes_filter {
            if let Some(c) = counter {
                c.fetch_add(1, Ordering::Relaxed);
            }
        }

        Ok(Node::File {
            name: name.clone(),
            path: path.to_path_buf(),
            size_mb,
            size_bytes,
            extension,
            created: metadata.created().ok().and_then(to_datetime),
            modified: metadata.modified().ok().and_then(to_datetime),
        })
    }
}

/// Aplana el árbol a una lista de FileRecord (para CSV).
/// El árbol ya viene filtrado por extensión desde `build_tree`,
/// así que aquí solo recorremos y serializamos. Es una operación en memoria
/// (el árbol ya fue construido), por lo que no reporta progreso.
pub fn flatten_to_records(node: &Node, out: &mut Vec<FileRecord>) {
    match node {
        Node::Directory { children, .. } => {
            for c in children {
                flatten_to_records(c, out);
            }
        }
        Node::File {
            path,
            size_mb,
            size_bytes,
            extension,
            created,
            modified,
            ..
        } => {
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
                size_bytes: *size_bytes,
                // Formato DD/MM/AAAA (solo fecha, sin hora) para Excel
                created: created
                    .map(|d| d.format("%d/%m/%Y").to_string())
                    .unwrap_or_default(),
                modified: modified
                    .map(|d| d.format("%d/%m/%Y").to_string())
                    .unwrap_or_default(),
                // ISO-8601 completo (fecha + hora) para ordenar/procesar
                created_iso: created.map(|d| d.to_rfc3339()).unwrap_or_default(),
                modified_iso: modified.map(|d| d.to_rfc3339()).unwrap_or_default(),
                directory: dir,
                full_path: path.display().to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn node_names(node: &Node) -> Vec<String> {
        match node {
            Node::Directory { children, .. } => {
                children.iter().map(|c| c.name().to_string()).collect()
            }
            Node::File { .. } => vec![],
        }
    }

    #[test]
    fn build_tree_filters_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.jpg"), "hello").unwrap();
        fs::write(dir.path().join("c.TXT"), "hello").unwrap();

        let filters = vec!["txt".to_string()];
        let tree = build_tree(dir.path(), &filters, None, &[], None, None).unwrap();

        let mut records = Vec::new();
        flatten_to_records(&tree, &mut records);
        let mut names: Vec<String> = records.iter().map(|r| r.name.clone()).collect();
        names.sort();

        // El filtro es case-insensitive: "c.TXT" también debe pasar.
        assert_eq!(names, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn build_tree_drops_empty_dirs_after_filtering() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("only_jpgs");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("pic.jpg"), "x").unwrap();
        fs::write(dir.path().join("doc.txt"), "x").unwrap();

        let filters = vec!["txt".to_string()];
        let tree = build_tree(dir.path(), &filters, None, &[], None, None).unwrap();

        // "only_jpgs" queda vacía tras filtrar por txt, así que se descarta
        // del árbol en vez de aparecer como carpeta sin contenido.
        assert_eq!(node_names(&tree), vec!["doc.txt".to_string()]);
    }

    #[test]
    fn build_tree_sorts_dirs_before_files_alphabetically() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("zeta.txt"), "x").unwrap();
        fs::create_dir(dir.path().join("alpha_dir")).unwrap();
        fs::write(dir.path().join("alpha_dir").join("inner.txt"), "x").unwrap();
        fs::write(dir.path().join("beta.txt"), "x").unwrap();

        let tree = build_tree(dir.path(), &[], None, &[], None, None).unwrap();
        assert_eq!(
            node_names(&tree),
            vec![
                "alpha_dir".to_string(),
                "beta.txt".to_string(),
                "zeta.txt".to_string(),
            ]
        );
    }

    #[test]
    fn flatten_to_records_counts_and_fills_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("one.txt"), "12345").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("two.txt"), "abcdefgh").unwrap();

        let tree = build_tree(dir.path(), &[], None, &[], None, None).unwrap();
        let mut records = Vec::new();
        flatten_to_records(&tree, &mut records);

        assert_eq!(records.len(), 2);
        let one = records.iter().find(|r| r.name == "one").unwrap();
        assert_eq!(one.extension, "txt");
        assert_eq!(one.size_bytes, 5);
        // ISO-8601 debe poder parsearse de vuelta (ordenable, con hora).
        assert!(chrono::DateTime::parse_from_rfc3339(&one.modified_iso).is_ok());
        assert!(chrono::DateTime::parse_from_rfc3339(&one.created_iso).is_ok());
        let two = records.iter().find(|r| r.name == "two").unwrap();
        assert_eq!(two.size_bytes, 8);
        assert!(two.full_path.ends_with("two.txt"));
    }
}
