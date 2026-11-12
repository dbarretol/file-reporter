use crate::models::{ExtStat, FileRecord, Node};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tracing::{debug, info, warn};

/// `true` si dos nombres o rutas designan lo mismo en el sistema de archivos.
///
/// En Windows (NTFS) y macOS (APFS/HFS+ por defecto), el sistema de
/// archivos es case-insensitive pero case-preserving: si el usuario tecleó
/// la ruta de entrada y la de salida con distinta capitalización para la
/// misma carpeta física (p. ej. `C:\Data` vs `c:\data`), las rutas
/// excluidas construidas a partir de una (los report.csv/md/json propios)
/// no coincidirían con las rutas descubiertas al recorrer la otra si se
/// comparan de forma exacta, y una corrida terminaría incluyéndose a sí
/// misma en su propio reporte. Se pliega con `to_lowercase` de Unicode (no
/// solo ASCII) porque NTFS es case-insensitive a nivel Unicode: una ruta con
/// `Ñ`/`á` tecleada con otra caja debe seguir coincidiendo. Se compara
/// carácter a carácter, sin reservar memoria: se llama una vez por cada
/// entrada del árbol. En otros sistemas de archivos (ext4 en Linux,
/// típicamente case-sensitive) sí puede haber dos archivos distintos que
/// solo difieran en mayúsculas, así que ahí la comparación es exacta.
#[cfg(any(windows, target_os = "macos"))]
fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn same_name(a: &str, b: &str) -> bool {
    a == b
}

/// Rutas exactas que se omiten del escaneo (los reportes propios y, si es
/// distinta de la entrada, la carpeta de salida).
struct Exclusions {
    /// `(nombre final, ruta completa)` de cada exclusión. El nombre permite
    /// descartar casi todas las entradas con una comparación corta, sin
    /// construir ni comparar su ruta completa.
    entries: Vec<(String, String)>,
}

impl Exclusions {
    fn new(paths: &[PathBuf]) -> Self {
        let entries = paths
            .iter()
            .map(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (name, p.to_string_lossy().into_owned())
            })
            .collect();
        Self { entries }
    }

    fn contains(&self, name: &str, path: &Path) -> bool {
        self.entries.iter().any(|(ex_name, ex_path)| {
            same_name(name, ex_name) && same_name(&path.to_string_lossy(), ex_path)
        })
    }
}

/// Opciones de `build_tree`. `ScanOptions::default()` recorre todo, sin
/// filtros ni exclusiones, y sin informar de progreso.
#[derive(Default)]
pub struct ScanOptions<'a> {
    /// Extensiones ya normalizadas (minúsculas, sin punto; ver
    /// `runner::parse_extensions`). Vacío = todas.
    pub extensions: &'a [String],
    /// Solo la estructura de carpetas, sin archivos.
    pub dirs_only: bool,
    /// Rutas exactas que se omiten (típicamente los reportes de la carpeta
    /// de salida), para que una corrida no se incluya a sí misma ni a la
    /// anterior.
    pub excluded_paths: &'a [PathBuf],
    /// Nombres de carpeta que se omiten, con todo su contenido, en cualquier
    /// nivel por debajo de la raíz (p. ej. `.git`, `node_modules`). Se
    /// comparan igual que las rutas (ver `same_name`).
    pub excluded_dir_names: &'a [String],
    /// Se incrementa por cada archivo que pasa el filtro (o por cada
    /// carpeta, en modo solo carpetas), a medida que se procesa, para
    /// informar del progreso (GUI o indicador de la CLI).
    pub counter: Option<&'a AtomicUsize>,
    /// Si pasa a `true`, el recorrido se aborta con error en vez de seguir
    /// leyendo disco (cancelación pedida por el usuario).
    pub cancel: Option<&'a AtomicBool>,
    /// Se incrementa por cada carpeta o archivo que no se pudo leer
    /// (permisos, entrada corrupta, etc.), para avisar de que el reporte
    /// quedó incompleto. No cuenta los symlinks, las exclusiones ni la
    /// cancelación, que son omisiones intencionales.
    pub skipped: Option<&'a AtomicUsize>,
}

/// Parámetros del recorrido, iguales para toda la recursión.
struct Scan<'a> {
    filters: &'a [String],
    dirs_only: bool,
    counter: Option<&'a AtomicUsize>,
    exclusions: Exclusions,
    excluded_dir_names: &'a [String],
    cancel: Option<&'a AtomicBool>,
    skipped: Option<&'a AtomicUsize>,
}

impl Scan<'_> {
    fn bump(counter: Option<&AtomicUsize>) {
        if let Some(c) = counter {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    fn passes_filter(&self, extension: &str) -> bool {
        self.filters.is_empty() || self.filters.iter().any(|f| f == extension)
    }

    fn is_excluded_dir(&self, name: &str) -> bool {
        self.excluded_dir_names.iter().any(|n| same_name(name, n))
    }
}

/// Construye recursivamente el árbol jerárquico (ver `ScanOptions`).
/// Los errores de lectura (permisos, etc.) se ignoran por archivo/carpeta
/// para que el escaneo sea robusto, y se cuentan en `skipped`.
///
/// Rendimiento: el tipo de cada entrada y los metadatos de los archivos salen
/// de la propia lectura del directorio (`DirEntry::file_type`/`metadata`),
/// que en Windows no requiere abrir cada archivo. Los archivos que no pasan
/// el filtro, o todos en modo solo carpetas, se descartan antes de pedir sus
/// metadatos, y las carpetas excluidas por nombre no se llegan a abrir. Las
/// subcarpetas se recorren en paralelo.
///
/// Recursión y límite de pila: cada nivel de profundidad del árbol añade un
/// marco a la pila del hilo que lo recorre (el propio `rayon::par_iter` de
/// más abajo, por work-stealing, también ejecuta parte de la recursión en el
/// hilo que la inicia, no solo en hilos "hijos"). `main.rs` configura tanto
/// el pool global de rayon como el hilo worker de la GUI con 16 MB de pila
/// (8x el valor por defecto de Rust) para dar margen amplio ante anidamientos
/// extremos (rutas largas `\\?\` de Windows, etc.). Un recorrido iterativo
/// con pila explícita eliminaría el límite por completo, pero se descarta por
/// ahora: es un riesgo bajo y una pila de 16 MB ya cubre profundidades muy
/// por encima de cualquier estructura de archivos real.
pub fn build_tree(path: &Path, opts: &ScanOptions<'_>) -> Result<Node> {
    let scan = Scan {
        filters: opts.extensions,
        dirs_only: opts.dirs_only,
        counter: opts.counter,
        exclusions: Exclusions::new(opts.excluded_paths),
        excluded_dir_names: opts.excluded_dir_names,
        cancel: opts.cancel,
        skipped: opts.skipped,
    };
    if scan.cancelled() {
        bail!("Escaneo cancelado");
    }

    // La raíz la eligió el usuario a propósito (selector de carpeta o drag &
    // drop): si resulta ser un symlink/junction (p. ej. una carpeta de
    // OneDrive) se sigue, en vez de rechazarla. La protección contra ciclos
    // solo hace falta para los descendientes, que nunca se siguen (ver
    // `scan_dir`). `fs::metadata` sigue el enlace; `symlink_metadata`, no.
    let metadata = fs::metadata(path)
        .with_context(|| format!("No se pudo leer metadatos de: {}", path.display()))?;

    if metadata.is_dir() {
        scan_dir(&scan, path, true)
    } else {
        let name = display_name(path);
        Ok(file_node(name, path.to_path_buf(), &metadata))
    }
}

/// Nombre a mostrar de una ruta: su último componente o, si no tiene (p. ej.
/// la raíz de una unidad, `C:\`), la ruta completa.
fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn file_node(name: String, path: PathBuf, metadata: &fs::Metadata) -> Node {
    let size_bytes = metadata.len();
    let extension = file_extension(&name);
    Node::File {
        name,
        path,
        size_mb: size_bytes as f64 / (1024.0 * 1024.0),
        size_bytes,
        extension,
        created: metadata.created().ok().map(DateTime::<Utc>::from),
        modified: metadata.modified().ok().map(DateTime::<Utc>::from),
    }
}

/// Extensión en minúsculas: lo que va tras el último punto del nombre
/// (`foo.tar.gz` → `gz`); vacía si no hay punto o el nombre empieza por él
/// (`.gitignore`), igual que `Path::extension`.
fn file_extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Recorre una carpeta y, en paralelo, sus subcarpetas.
fn scan_dir(scan: &Scan<'_>, path: &Path, is_root: bool) -> Result<Node> {
    if scan.cancelled() {
        bail!("Escaneo cancelado");
    }

    // En modo solo carpetas el progreso cuenta carpetas (la raíz no, igual
    // que en `count_dirs`), para que el contador en vivo coincida con el
    // total final del resumen.
    if scan.dirs_only && !is_root {
        Scan::bump(scan.counter);
    }

    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut children: Vec<Node> = Vec::new();

    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(e) => {
                        warn!("Entrada ilegible en {}: {e}", path.display());
                        Scan::bump(scan.skipped);
                        continue;
                    }
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                let entry_path = entry.path();
                if scan.exclusions.contains(&name, &entry_path) {
                    debug!(
                        "Omitido (reporte o carpeta de salida): {}",
                        entry_path.display()
                    );
                    continue;
                }
                let file_type = match entry.file_type() {
                    Ok(t) => t,
                    Err(e) => {
                        warn!("Sin acceso a {}: {e}", entry_path.display());
                        Scan::bump(scan.skipped);
                        continue;
                    }
                };
                if file_type.is_symlink() {
                    // Symlinks y junctions descendientes no se siguen: evita
                    // recursión infinita en ciclos. Omisión intencional, no
                    // cuenta como error.
                    info!("Enlace no seguido: {}", entry_path.display());
                    continue;
                }
                if file_type.is_dir() {
                    // Excluida por nombre (`.git`, `node_modules`…): ni se
                    // abre. Omisión intencional, no cuenta como error.
                    if scan.is_excluded_dir(&name) {
                        info!("Carpeta omitida por nombre: {}", entry_path.display());
                    } else {
                        subdirs.push(entry_path);
                    }
                    continue;
                }
                // Archivo: en modo solo carpetas no aparece en el reporte, y
                // con filtro solo interesan los que pasan; en ambos casos se
                // descarta sin pedir sus metadatos.
                if scan.dirs_only || !scan.passes_filter(&file_extension(&name)) {
                    continue;
                }
                match entry.metadata() {
                    Ok(metadata) => {
                        Scan::bump(scan.counter);
                        children.push(file_node(name, entry_path, &metadata));
                    }
                    Err(e) => {
                        warn!("Sin acceso a {}: {e}", entry_path.display());
                        Scan::bump(scan.skipped);
                    }
                }
            }
        }
        Err(e) => {
            // Sin permisos -> no podemos listar el contenido; queda como
            // carpeta vacía en el árbol, pero contamos la pérdida.
            warn!("No se pudo leer la carpeta {}: {e}", path.display());
            Scan::bump(scan.skipped);
        }
    }

    let dirs: Vec<Node> = subdirs
        .par_iter()
        .filter_map(|p| scan_dir(scan, p, false).ok())
        // Con filtro activo, una carpeta que quedó vacía tras filtrar se
        // descarta en vez de aparecer sin contenido. En modo solo carpetas
        // se conservan todas.
        .filter(|n| match n {
            Node::Directory { children, .. } => {
                scan.dirs_only || scan.filters.is_empty() || !children.is_empty()
            }
            Node::File { .. } => true,
        })
        .collect();
    children.extend(dirs);

    // Orden alfabético sin distinguir mayúsculas: directorios primero, luego
    // archivos. La clave se calcula una vez por elemento, no en cada
    // comparación.
    children.sort_by_cached_key(|n| (!n.is_dir(), n.name().to_lowercase()));

    Ok(Node::Directory {
        name: display_name(path),
        path: path.to_path_buf(),
        children,
    })
}

/// Cuenta las carpetas del árbol, sin incluir la raíz.
pub fn count_dirs(node: &Node) -> usize {
    match node {
        Node::Directory { children, .. } => children
            .iter()
            .filter(|c| c.is_dir())
            .map(|c| 1 + count_dirs(c))
            .sum(),
        Node::File { .. } => 0,
    }
}

/// Número de archivos del árbol y suma de sus tamaños en bytes.
pub fn file_totals(node: &Node) -> (usize, u64) {
    match node {
        Node::Directory { children, .. } => children
            .iter()
            .map(file_totals)
            .fold((0, 0), |acc, x| (acc.0 + x.0, acc.1 + x.1)),
        Node::File { size_bytes, .. } => (1, *size_bytes),
    }
}

/// Cantidad y tamaño de los archivos del árbol agrupados por extensión,
/// de mayor a menor tamaño total (a igual tamaño, más archivos primero y
/// luego por nombre, para que el orden sea estable entre corridas).
pub fn extension_stats(node: &Node) -> Vec<ExtStat> {
    fn walk<'a>(node: &'a Node, acc: &mut HashMap<&'a str, (usize, u64)>) {
        match node {
            Node::Directory { children, .. } => children.iter().for_each(|c| walk(c, acc)),
            Node::File {
                extension,
                size_bytes,
                ..
            } => {
                let e = acc.entry(extension.as_str()).or_default();
                e.0 += 1;
                e.1 += size_bytes;
            }
        }
    }
    let mut acc = HashMap::new();
    walk(node, &mut acc);
    let mut stats: Vec<ExtStat> = acc
        .into_iter()
        .map(|(ext, (count, bytes))| ExtStat {
            extension: ext.to_string(),
            count,
            bytes,
        })
        .collect();
    stats.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then(b.count.cmp(&a.count))
            .then_with(|| a.extension.cmp(&b.extension))
    });
    stats
}

/// Recorre los archivos del árbol, en orden, y llama a `f` con el
/// `FileRecord` (fila del CSV) de cada uno. Se detiene en el primer error.
///
/// Las filas se generan de una en una en vez de acumularlas en un `Vec`: con
/// cientos de miles de archivos, esa lista duplicaba en memoria lo que ya
/// guarda el árbol. El árbol ya viene filtrado desde `build_tree`.
pub fn try_for_each_record(
    node: &Node,
    f: &mut impl FnMut(FileRecord) -> Result<()>,
) -> Result<()> {
    match node {
        Node::Directory { children, .. } => {
            for c in children {
                try_for_each_record(c, f)?;
            }
            Ok(())
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

            f(FileRecord {
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
            })
        }
    }
}

/// Todas las filas del CSV en una lista (solo para tests).
#[cfg(test)]
pub fn flatten_to_records(node: &Node, out: &mut Vec<FileRecord>) {
    try_for_each_record(node, &mut |r| {
        out.push(r);
        Ok(())
    })
    .expect("el visitante no falla");
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
        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                extensions: &filters,
                ..Default::default()
            },
        )
        .unwrap();

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
        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                extensions: &filters,
                ..Default::default()
            },
        )
        .unwrap();

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

        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                dirs_only: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            node_names(&tree),
            vec!["folder1".to_string(), "folder2".to_string()]
        );
        let mut records = Vec::new();
        flatten_to_records(&tree, &mut records);
        assert!(records.is_empty());
        assert_eq!(count_dirs(&tree), 2);
    }

    #[test]
    fn build_tree_counter_counts_only_files_that_pass_the_filter() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        fs::write(dir.path().join("b.jpg"), "x").unwrap();
        fs::write(dir.path().join("sub").join("c.txt"), "x").unwrap();

        let counter = AtomicUsize::new(0);
        let filters = vec!["txt".to_string()];
        build_tree(
            dir.path(),
            &ScanOptions {
                extensions: &filters,
                counter: Some(&counter),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(counter.load(Ordering::Relaxed), 2);

        // En modo solo carpetas cuenta carpetas (sin la raíz), no archivos.
        let counter = AtomicUsize::new(0);
        build_tree(
            dir.path(),
            &ScanOptions {
                dirs_only: true,
                counter: Some(&counter),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn build_tree_excludes_exact_paths_not_every_entry_with_that_name() {
        // Se excluye `report.csv` de la raíz (el reporte propio), pero no un
        // `report.csv` de otra carpeta: el atajo por nombre no debe bastar.
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(dir.path().join("report.csv"), "x").unwrap();
        fs::write(sub.join("report.csv"), "x").unwrap();

        let excluded = vec![dir.path().join("report.csv")];
        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                excluded_paths: &excluded,
                ..Default::default()
            },
        )
        .unwrap();

        let mut records = Vec::new();
        flatten_to_records(&tree, &mut records);
        assert_eq!(records.len(), 1);
        assert!(records[0].full_path.contains("sub"));
    }

    #[test]
    fn build_tree_sorts_dirs_before_files_alphabetically() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("zeta.txt"), "x").unwrap();
        fs::create_dir(dir.path().join("alpha_dir")).unwrap();
        fs::write(dir.path().join("alpha_dir").join("inner.txt"), "x").unwrap();
        fs::write(dir.path().join("beta.txt"), "x").unwrap();

        let tree = build_tree(dir.path(), &ScanOptions::default()).unwrap();
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
        // separadas con distinto casing).
        let mixed_case_excluded = PathBuf::from(
            dir.path()
                .join("report.csv")
                .to_string_lossy()
                .to_uppercase(),
        );

        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                excluded_paths: &[mixed_case_excluded],
                ..Default::default()
            },
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
        // importar la caja usada.
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("AÑO2026");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("report.csv"), "x").unwrap();
        fs::write(sub.join("keep.txt"), "x").unwrap();

        let mixed_case_excluded = dir.path().join("año2026").join("report.csv");

        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                excluded_paths: &[mixed_case_excluded],
                ..Default::default()
            },
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
        // a propósito. Ahora debe seguirla y escanear el
        // contenido real.
        let link_root = dir.path().join("link_root");
        make_junction(&link_root, &real_root);
        let tree = build_tree(&link_root, &ScanOptions::default()).unwrap();
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

        let tree2 = build_tree(&outer, &ScanOptions::default()).unwrap();
        assert!(node_names(&tree2).is_empty());
    }

    #[test]
    fn flatten_to_records_counts_and_fills_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("one.txt"), "12345").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("two.txt"), "abcdefgh").unwrap();

        let tree = build_tree(dir.path(), &ScanOptions::default()).unwrap();
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

    #[test]
    fn build_tree_skips_excluded_dir_names_at_any_level() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".git").join("objects")).unwrap();
        fs::write(root.join(".git").join("HEAD"), "x").unwrap();
        fs::create_dir_all(root.join("web").join("node_modules").join("pkg")).unwrap();
        fs::write(
            root.join("web")
                .join("node_modules")
                .join("pkg")
                .join("i.js"),
            "x",
        )
        .unwrap();
        fs::write(root.join("web").join("app.js"), "x").unwrap();
        // Un archivo con el mismo nombre no es una carpeta: se conserva.
        fs::write(root.join("node_modules"), "x").unwrap();

        let names = vec![".git".to_string(), "node_modules".to_string()];
        let skipped = AtomicUsize::new(0);
        let tree = build_tree(
            root,
            &ScanOptions {
                excluded_dir_names: &names,
                skipped: Some(&skipped),
                ..Default::default()
            },
        )
        .unwrap();

        let mut records = Vec::new();
        flatten_to_records(&tree, &mut records);
        let mut paths: Vec<String> = records.iter().map(|r| r.full_path.clone()).collect();
        paths.sort();
        assert_eq!(paths.len(), 2, "{paths:?}");
        assert!(paths.iter().any(|p| p.ends_with("app.js")));
        assert!(paths.iter().any(|p| p.ends_with("node_modules")));
        assert_eq!(count_dirs(&tree), 1, "solo queda `web`");
        // Omitir a propósito no es un fallo de lectura.
        assert_eq!(skipped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn build_tree_never_excludes_the_root_itself() {
        // Si el usuario elige escanear `.git`, se escanea: la exclusión por
        // nombre solo afecta a lo que hay debajo.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".git");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("HEAD"), "x").unwrap();

        let names = vec![".git".to_string()];
        let tree = build_tree(
            &root,
            &ScanOptions {
                excluded_dir_names: &names,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(node_names(&tree), vec!["HEAD".to_string()]);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn build_tree_matches_excluded_dir_names_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("Node_Modules")).unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();

        let names = vec!["node_modules".to_string()];
        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                dirs_only: true,
                excluded_dir_names: &names,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(node_names(&tree), vec!["src".to_string()]);
    }

    #[test]
    fn extension_stats_groups_counts_and_sorts_by_size() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("a.txt"), "12345").unwrap();
        fs::write(dir.path().join("sub").join("b.TXT"), "123").unwrap();
        fs::write(dir.path().join("c.pdf"), "1234567890").unwrap();
        fs::write(dir.path().join("README"), "1").unwrap();
        fs::write(dir.path().join("d.rs"), "1").unwrap();

        let tree = build_tree(dir.path(), &ScanOptions::default()).unwrap();
        let stats = extension_stats(&tree);
        let rows: Vec<(&str, usize, u64)> = stats
            .iter()
            .map(|s| (s.extension.as_str(), s.count, s.bytes))
            .collect();
        // pdf (10 B) > txt (8 B, mayúsculas agrupadas); empate de 1 B
        // entre "" y "rs" → orden alfabético.
        assert_eq!(
            rows,
            vec![("pdf", 1, 10), ("txt", 2, 8), ("", 1, 1), ("rs", 1, 1)]
        );
        assert_eq!(stats[2].label(), "(sin extensión)");
        assert_eq!(stats[0].label(), ".pdf");
    }

    #[test]
    fn extension_stats_is_empty_in_dirs_only_mode() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let tree = build_tree(
            dir.path(),
            &ScanOptions {
                dirs_only: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(extension_stats(&tree).is_empty());
    }
}
