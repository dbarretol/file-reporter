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

/// `true` si `p` coincide con alguna ruta de `excluded`.
///
/// En Windows (NTFS) y macOS (APFS/HFS+ por defecto), el sistema de
/// archivos es case-insensitive pero case-preserving: si el usuario tecleó
/// la ruta de entrada y la de salida con distinta capitalización para la
/// misma carpeta física (p. ej. `C:\Data` vs `c:\data`), las rutas
/// excluidas construidas a partir de una (los report.csv/md/json propios)
/// no coincidirían con las rutas descubiertas al recorrer la otra si se
/// comparan con `==` (case-sensitive, como hace `PathBuf`), y una corrida
/// terminaría incluyéndose a sí misma en su propio reporte (reintroduce el
/// bug #4 — ver tarea 28 en tasks.md). `to_lowercase()` (no
/// `eq_ignore_ascii_case`) porque NTFS pliega mayúsculas/minúsculas a nivel
/// Unicode, no solo ASCII: una ruta con `Ñ`/`á` tecleada con otra caja debe
/// seguir coincidiendo (ver tarea 35). En otros sistemas de archivos (ext4
/// en Linux, típicamente case-sensitive) sí puede haber dos archivos
/// distintos que solo difieran en mayúsculas, así que ahí se mantiene la
/// comparación exacta.
#[cfg(any(windows, target_os = "macos"))]
fn path_is_excluded(p: &Path, excluded: &[PathBuf]) -> bool {
    let p_lower = p.to_string_lossy().to_lowercase();
    excluded
        .iter()
        .any(|e| e.to_string_lossy().to_lowercase() == p_lower)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn path_is_excluded(p: &Path, excluded: &[PathBuf]) -> bool {
    excluded.contains(&p.to_path_buf())
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
///
/// Recursión y límite de pila: cada nivel de profundidad del árbol añade un
/// marco a la pila del hilo que lo recorre (el propio `rayon::par_iter` de
/// más abajo, por work-stealing, también ejecuta parte de la recursión en el
/// hilo que la inicia, no solo en hilos "hijos"). `main.rs` configura tanto
/// el pool global de rayon como el hilo worker de la GUI con 16 MB de pila
/// (8x el valor por defecto de Rust) para dar margen amplio ante anidamientos
/// extremos (rutas largas `\\?\` de Windows, symlinks NTFS anidados antes de
/// resolverse, etc.). Un recorrido iterativo con pila explícita eliminaría el
/// límite por completo, pero se descarta por ahora: la auditoría marca este
/// punto como bajo riesgo y una pila de 16 MB ya cubre profundidades muy por
/// encima de cualquier estructura de archivos real.
pub fn build_tree(
    path: &Path,
    filters: &[String],
    dirs_only: bool,
    counter: Option<&AtomicUsize>,
    excluded: &[PathBuf],
    cancel: Option<&AtomicBool>,
    skipped: Option<&AtomicUsize>,
) -> Result<Node> {
    build_tree_inner(
        path, filters, dirs_only, counter, excluded, cancel, skipped, true,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_tree_inner(
    path: &Path,
    filters: &[String],
    dirs_only: bool,
    counter: Option<&AtomicUsize>,
    excluded: &[PathBuf],
    cancel: Option<&AtomicBool>,
    skipped: Option<&AtomicUsize>,
    is_root: bool,
) -> Result<Node> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        bail!("Escaneo cancelado");
    }

    // symlink_metadata (a diferencia de metadata) no sigue enlaces simbólicos
    // ni junctions de Windows, evitando recursión infinita en ciclos.
    let mut metadata = match fs::symlink_metadata(path) {
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
        if is_root {
            // La raíz la eligió el usuario a propósito (selector de carpeta o
            // drag & drop): rechazarla porque resulta ser un symlink/junction
            // (p. ej. una carpeta de OneDrive) es una sorpresa confusa, no
            // una protección útil. La protección contra ciclos solo hace
            // falta para los descendientes (rama `else`, sin cambios) — ver
            // tarea 29 en tasks.md. `fs::metadata` sí sigue el enlace, a
            // diferencia de `symlink_metadata`.
            metadata = fs::metadata(path).with_context(|| {
                format!(
                    "No se pudo resolver el enlace de la carpeta raíz: {}",
                    path.display()
                )
            })?;
        } else {
            bail!("Enlace simbólico omitido: {}", path.display());
        }
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
                .filter(|p| !path_is_excluded(p, excluded))
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
            .filter_map(|p| {
                build_tree_inner(
                    p, filters, dirs_only, counter, excluded, cancel, skipped, false,
                )
                .ok()
            })
            .filter(|n| match n {
                // Conservamos archivos que pasen el filtro (y si no se pidió solo directorios)
                Node::File { extension, .. } => {
                    !dirs_only
                        && (filters.is_empty()
                            || filters.iter().any(|f| f.eq_ignore_ascii_case(extension)))
                }
                // Conservamos directorios
                Node::Directory { children, .. } => {
                    if dirs_only {
                        true
                    } else {
                        !children.is_empty() || filters.is_empty()
                    }
                }
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
        if dirs_only {
            // Los archivos no aparecen en el árbol/reporte final en este modo,
            // pero seguimos incrementando el contador de progreso al tocarlos
            // (mismo coste de I/O que el modo normal): si no, la GUI se queda
            // congelada en "0 archivos encontrados…" durante todo el escaneo,
            // sin ninguna señal de que sigue trabajando.
            if let Some(c) = counter {
                c.fetch_add(1, Ordering::Relaxed);
            }
            return Ok(Node::File {
                name: name.clone(),
                path: path.to_path_buf(),
                size_mb: 0.0,
                size_bytes: 0,
                extension: String::new(),
                created: None,
                modified: None,
            });
        }

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
        let tree = build_tree(dir.path(), &filters, false, None, &[], None, None).unwrap();

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
        let tree = build_tree(dir.path(), &filters, false, None, &[], None, None).unwrap();

        // "only_jpgs" queda vacía tras filtrar por txt, así que se descarta
        // del árbol en vez de aparecer como carpeta sin contenido.
        assert_eq!(node_names(&tree), vec!["doc.txt".to_string()]);
    }

    #[test]
    fn build_tree_dirs_only_includes_only_folders() {
        let dir = tempfile::tempdir().unwrap();
        let sub1 = dir.path().join("folder1");
        let sub2 = dir.path().join("folder2");
        fs::create_dir(&sub1).unwrap();
        fs::create_dir(&sub2).unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();
        fs::write(sub1.join("inner.txt"), "x").unwrap();

        let tree = build_tree(dir.path(), &[], true, None, &[], None, None).unwrap();

        assert_eq!(
            node_names(&tree),
            vec!["folder1".to_string(), "folder2".to_string()]
        );
        let mut records = Vec::new();
        flatten_to_records(&tree, &mut records);
        assert!(records.is_empty());
    }

    #[test]
    fn build_tree_sorts_dirs_before_files_alphabetically() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("zeta.txt"), "x").unwrap();
        fs::create_dir(dir.path().join("alpha_dir")).unwrap();
        fs::write(dir.path().join("alpha_dir").join("inner.txt"), "x").unwrap();
        fs::write(dir.path().join("beta.txt"), "x").unwrap();

        let tree = build_tree(dir.path(), &[], false, None, &[], None, None).unwrap();
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
    #[cfg(any(windows, target_os = "macos"))]
    fn build_tree_excludes_paths_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("report.csv"), "x").unwrap();
        fs::write(dir.path().join("keep.txt"), "x").unwrap();

        // Simula al usuario tecleando la ruta de salida con una
        // capitalización distinta a la de disco (misma carpeta física,
        // p. ej. por haber escrito entrada y salida en cajas de texto
        // separadas con distinto casing). Ver tarea 28.
        let mixed_case_excluded = PathBuf::from(
            dir.path()
                .join("report.csv")
                .to_string_lossy()
                .to_uppercase(),
        );

        let tree = build_tree(
            dir.path(),
            &[],
            false,
            None,
            &[mixed_case_excluded],
            None,
            None,
        )
        .unwrap();

        assert_eq!(node_names(&tree), vec!["keep.txt".to_string()]);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn build_tree_excludes_paths_case_insensitively_for_non_ascii_chars() {
        // `eq_ignore_ascii_case` (la implementación anterior) no pliega
        // caracteres no-ASCII; NTFS sí es case-insensitive a nivel Unicode,
        // así que una carpeta con "Ñ"/"año" debe excluirse igual sin
        // importar la caja usada (ver tarea 35).
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("AÑO2026");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("report.csv"), "x").unwrap();
        fs::write(sub.join("keep.txt"), "x").unwrap();

        let mixed_case_excluded = dir.path().join("año2026").join("report.csv");

        let tree = build_tree(
            dir.path(),
            &[],
            false,
            None,
            &[mixed_case_excluded],
            None,
            None,
        )
        .unwrap();

        let mut records = Vec::new();
        flatten_to_records(&tree, &mut records);
        let names: Vec<String> = records.iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, vec!["keep".to_string()]);
    }

    #[test]
    #[cfg(windows)]
    fn build_tree_follows_symlink_at_root_but_not_at_descendant() {
        // Las junctions no requieren privilegios elevados en Windows (a
        // diferencia de los symlinks reales), así que son la forma más
        // fiable de simular esto en un test sin depender de que la máquina
        // tenga modo desarrollador o se ejecute como admin.
        fn make_junction(link: &Path, target: &Path) {
            let status = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .status()
                .unwrap();
            assert!(status.success(), "no se pudo crear la junction de prueba");
        }

        let dir = tempfile::tempdir().unwrap();
        let real_root = dir.path().join("real_root");
        fs::create_dir(&real_root).unwrap();
        fs::write(real_root.join("a.txt"), "hello").unwrap();

        // Caso 1: la raíz misma es una junction. Antes del fix, `build_tree`
        // fallaba con "Enlace simbólico omitido" aunque el usuario la eligió
        // a propósito (ver tarea 29). Ahora debe seguirla y escanear el
        // contenido real.
        let link_root = dir.path().join("link_root");
        make_junction(&link_root, &real_root);
        let tree = build_tree(&link_root, &[], false, None, &[], None, None).unwrap();
        assert_eq!(node_names(&tree), vec!["a.txt".to_string()]);

        // Caso 2: una junction dentro del árbol (no en la raíz) se sigue
        // rechazando — la protección contra ciclos infinitos solo se relajó
        // para el nivel raíz.
        let real_sub = dir.path().join("real_sub");
        fs::create_dir(&real_sub).unwrap();
        fs::write(real_sub.join("b.txt"), "hello").unwrap();
        let outer = dir.path().join("outer");
        fs::create_dir(&outer).unwrap();
        make_junction(&outer.join("link_sub"), &real_sub);

        let tree2 = build_tree(&outer, &[], false, None, &[], None, None).unwrap();
        assert!(node_names(&tree2).is_empty());
    }

    #[test]
    fn flatten_to_records_counts_and_fills_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("one.txt"), "12345").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("two.txt"), "abcdefgh").unwrap();

        let tree = build_tree(dir.path(), &[], false, None, &[], None, None).unwrap();
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
