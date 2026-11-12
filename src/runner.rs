use crate::models::ExtStat;
use crate::reporters;
use crate::scanner::{self, ScanLog, ScanOptions, SkippedItem};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

/// Formato de reporte que se puede generar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ReportFormat {
    Csv,
    #[value(alias = "markdown")]
    Md,
    Json,
}

impl ReportFormat {
    /// Todos los formatos, en el orden en que se escriben y se muestran.
    pub const ALL: [ReportFormat; 3] = [ReportFormat::Csv, ReportFormat::Md, ReportFormat::Json];

    /// Extensión del archivo de este formato, sin punto.
    pub fn extension(self) -> &'static str {
        match self {
            ReportFormat::Csv => "csv",
            ReportFormat::Md => "md",
            ReportFormat::Json => "json",
        }
    }

    /// Nombre del archivo de este formato para el nombre base `base`
    /// (`report` → `report.csv`).
    pub fn file_name(self, base: &str) -> String {
        format!("{base}.{}", self.extension())
    }

    /// Nombre para mostrar al usuario.
    pub fn label(self) -> &'static str {
        match self {
            ReportFormat::Csv => "CSV",
            ReportFormat::Md => "Markdown",
            ReportFormat::Json => "JSON",
        }
    }
}

/// Parámetros de ejecución (válidos tanto para CLI como GUI).
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub extensions: Vec<String>,
    pub dirs_only: bool,
    /// Nombres de carpeta que se omiten en cualquier nivel (ver
    /// `parse_dir_names`).
    pub exclude_dirs: Vec<String>,
    /// Reportes a generar (al menos uno), sin repetidos.
    pub formats: Vec<ReportFormat>,
    /// Nombre base de los reportes, sin extensión (`report` → `report.csv`,
    /// `report.md`, `report.json`). Ver `validate_report_name`.
    pub report_name: String,
}

/// Nombre base de los reportes si no se elige otro.
pub const DEFAULT_REPORT_NAME: &str = "report";

/// Caracteres que no pueden ir en un nombre de archivo (en Windows; en
/// Linux y macOS solo `/`, pero así un mismo nombre vale en todos).
const INVALID_NAME_CHARS: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Comprueba que `name` sirva como nombre base de los reportes: no vacío,
/// sin separadores de carpeta ni caracteres no válidos en Windows, y sin
/// punto o espacio al final (Windows los quita en silencio).
pub fn validate_report_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("El nombre de los reportes no puede estar vacío");
    }
    if let Some(c) = name
        .chars()
        .find(|c| INVALID_NAME_CHARS.contains(c) || c.is_control())
    {
        bail!("El nombre de los reportes no puede contener «{c}»: {name}");
    }
    if name.ends_with(['.', ' ']) {
        bail!("El nombre de los reportes no puede terminar en punto ni en espacio: {name}");
    }
    Ok(())
}

/// Nombre alternativo para no pisar reportes existentes:
/// `report-<carpeta>-<AAAA-MM-DD>` y, si alguno de los `formats` ya existe
/// en `output` con ese nombre, `-2`, `-3`… La carpeta es el último
/// componente de `input` (la letra de unidad para `C:\\`), con los
/// caracteres no válidos cambiados por `_`.
pub fn auto_report_name(
    input: &Path,
    output: &Path,
    formats: &[ReportFormat],
    date: chrono::NaiveDate,
) -> String {
    let folder: String = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.display().to_string())
        .chars()
        .map(|c| {
            if INVALID_NAME_CHARS.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let folder = folder.trim_matches(['_', '.', ' ']);
    let folder = if folder.is_empty() { "carpeta" } else { folder };
    let base = format!("{DEFAULT_REPORT_NAME}-{folder}-{}", date.format("%Y-%m-%d"));
    let taken = |name: &str| {
        formats
            .iter()
            .any(|f| output.join(f.file_name(name)).exists())
    };
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|name| !taken(name))
        .expect("siempre hay un número libre")
}

/// `true` si `file_name` es un reporte con nombre automático (ver
/// `auto_report_name`) o su temporal: `report-<carpeta>-AAAA-MM-DD[-N]`
/// con extensión `.csv`, `.md` o `.json` (y quizá `.tmp`). Sirve para no
/// incluir en un reporte los de corridas anteriores guardadas con otro
/// nombre.
pub fn is_auto_report_name(file_name: &str) -> bool {
    let name = file_name.strip_suffix(".tmp").unwrap_or(file_name);
    let Some(stem) = ReportFormat::ALL
        .iter()
        .find_map(|f| name.strip_suffix(&format!(".{}", f.extension())))
    else {
        return false;
    };
    let Some(rest) = stem.strip_prefix(&format!("{DEFAULT_REPORT_NAME}-")) else {
        return false;
    };
    // `<carpeta>-AAAA-MM-DD`, con la carpeta no vacía.
    let ends_with_date = |s: &str| {
        s.len() > 11 && {
            let (folder, date) = s.split_at(s.len() - 11);
            !folder.is_empty()
                && date.starts_with('-')
                && chrono::NaiveDate::parse_from_str(&date[1..], "%Y-%m-%d").is_ok()
        }
    };
    if ends_with_date(rest) {
        return true;
    }
    // Con sufijo `-N`.
    match rest.rsplit_once('-') {
        Some((head, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            ends_with_date(head)
        }
        _ => false,
    }
}

impl RunConfig {
    /// Rutas de los reportes que generaría esta corrida, en el orden de
    /// `ReportFormat::ALL` y sin repetidos, se hayan pedido como se hayan
    /// pedido (p. ej. `-f json,csv,json`).
    pub fn report_paths(&self) -> Vec<(ReportFormat, PathBuf)> {
        ReportFormat::ALL
            .into_iter()
            .filter(|f| self.formats.contains(f))
            .map(|f| (f, self.output.join(f.file_name(&self.report_name))))
            .collect()
    }
}

/// Normaliza una lista de extensiones tal como la escribe el usuario, igual
/// en CLI y GUI: separa por comas, punto y coma o espacios, quita el punto
/// inicial, pasa a minúsculas y descarta vacíos y repetidos (conservando el
/// orden). Así `"pdf, .DOCX;jpg"` y `-e pdf -e " docx"` dan lo mismo.
pub fn parse_extensions<'a>(inputs: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for input in inputs {
        for token in input.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
            let ext = token.trim_start_matches('.').to_lowercase();
            if !ext.is_empty() && !out.contains(&ext) {
                out.push(ext);
            }
        }
    }
    out
}

/// Carpetas que suelen inflar el reporte de un proyecto de código sin
/// aportar nada: control de versiones, dependencias y artefactos de
/// compilación. Las ofrecen la opción `--exclude-common` de la CLI y la
/// sugerencia "Carpetas de proyectos" de la GUI, que además las omite de
/// partida.
pub const COMMON_EXCLUDED_DIRS: [&str; 7] = [
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "dist",
    "build",
];

/// Normaliza una lista de nombres de carpeta tal como la escribe el usuario,
/// igual en CLI y GUI: separa por comas o punto y coma (no por espacios: un
/// nombre de carpeta puede tenerlos), quita espacios a los lados y las
/// barras finales (`node_modules/`), y descarta vacíos y repetidos
/// (conservando el orden). Mayúsculas y minúsculas se respetan: la
/// comparación la decide el escáner según el sistema de archivos.
pub fn parse_dir_names<'a>(inputs: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for input in inputs {
        for token in input.split([',', ';']) {
            let name = token.trim().trim_end_matches(['/', '\\']).trim();
            if !name.is_empty() && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Progreso compartido entre `run` y quien lo muestra (GUI o spinner de la CLI).
#[derive(Debug, Default)]
pub struct Progress {
    /// Archivos (o carpetas, en modo solo carpetas) encontrados hasta ahora.
    pub done: AtomicUsize,
    /// Carpetas abiertas hasta ahora, hayan aportado algo al reporte o no.
    pub dirs_scanned: AtomicUsize,
    /// La GUI la pone en `true` para pedir cancelación.
    pub cancel: AtomicBool,
    /// `false` mientras se escanea el disco, `true` una vez terminado el
    /// escaneo y mientras se escriben report.csv/md/json. Permite que la
    /// GUI distinga la fase real en curso en vez de mostrar un genérico
    /// "Procesando…" durante todo el proceso.
    pub writing: AtomicBool,
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

/// Resultado de una ejecución exitosa.
#[derive(Debug, Clone)]
pub struct RunSummary {
    /// Reportes generados, en el orden de `ReportFormat::ALL`.
    pub reports: Vec<(ReportFormat, PathBuf)>,
    pub files_count: usize,
    /// Suma de los tamaños de los archivos incluidos en el reporte (0 en
    /// modo solo carpetas).
    pub total_bytes: u64,
    /// Carpetas del árbol reportado (sin contar la raíz).
    pub dirs_count: usize,
    /// Si la corrida fue en modo solo carpetas (para elegir qué conteo mostrar).
    pub dirs_only: bool,
    /// Carpetas/archivos que no se pudieron leer (permisos, etc.) y por lo
    /// tanto no aparecen en el reporte, aunque existan en disco.
    pub skipped_count: usize,
    /// Los primeros de esos elementos (hasta `scanner::SKIPPED_LIST_LIMIT`),
    /// con su motivo, ordenados por ruta.
    pub skipped: Vec<SkippedItem>,
    /// Carpetas omitidas a propósito por su nombre: `(nombre de la lista,
    /// cuántas)`, en orden alfabético. Vacío si no se omitió ninguna.
    pub excluded_dirs: Vec<(String, usize)>,
    /// Archivos agrupados por extensión, de mayor a menor tamaño (vacío en
    /// modo solo carpetas).
    pub ext_stats: Vec<ExtStat>,
}

/// Ejecuta el proceso completo: escanear + generar los reportes pedidos.
/// Reporta progreso si se pasa un `Progress`.
pub fn run(cfg: &RunConfig, progress: Option<Arc<Progress>>) -> Result<RunSummary> {
    if cfg.formats.is_empty() {
        bail!("No se eligió ningún formato de reporte");
    }
    if !cfg.input.exists() {
        bail!("La ruta no existe: {}", cfg.input.display());
    }
    if !cfg.input.is_dir() {
        bail!("La ruta no es un directorio: {}", cfg.input.display());
    }

    validate_report_name(&cfg.report_name)?;

    let reports = cfg.report_paths();

    // Excluir del escaneo los reportes de una corrida anterior (siempre, de
    // los tres formatos: también los que esta corrida no genera), con el
    // nombre por defecto y con el de esta corrida, sus temporales
    // (`report.csv.tmp`…, que quedarían si un corte de luz interrumpe la
    // escritura; ver `reporters::write_atomic`), los guardados con nombre
    // automático (`report-docs-2026-11-12.csv`…, ver `is_auto_report_name`)
    // y, si la salida es una carpeta distinta de la entrada, la carpeta de
    // salida completa (por si contiene sobras de corridas previas). Cuando
    // output == input no se puede excluir la carpeta entera, solo los
    // archivos exactos.
    let mut excluded: Vec<PathBuf> = [DEFAULT_REPORT_NAME, cfg.report_name.as_str()]
        .into_iter()
        .flat_map(|base| ReportFormat::ALL.map(|f| cfg.output.join(f.file_name(base))))
        .flat_map(|p| [reporters::temp_path(&p), p])
        .collect();
    // Una sola lectura de la carpeta de salida (si ya existe), no del árbol.
    if let Ok(entries) = std::fs::read_dir(&cfg.output) {
        excluded.extend(
            entries
                .flatten()
                .filter(|e| is_auto_report_name(&e.file_name().to_string_lossy()))
                .map(|e| e.path()),
        );
    }
    if cfg.output != cfg.input {
        excluded.push(cfg.output.clone());
    }

    let cancel_flag = progress.as_ref().map(|p| &p.cancel);
    let log = ScanLog::default();
    debug!(?cfg, "Inicio del escaneo");
    let scan_start = Instant::now();

    // No hacemos una pasada previa solo para contar archivos: eso implicaría
    // leer el disco dos veces (una para contar, otra para construir el
    // árbol), casi duplicando el tiempo total. El total se desconoce hasta
    // el final; la GUI y la CLI muestran el contador `done` en vivo y el
    // tiempo transcurrido.
    if let Some(p) = &progress {
        p.done.store(0, Ordering::Relaxed);
        p.dirs_scanned.store(0, Ordering::Relaxed);
    }

    // Única pasada por el disco: aquí ocurre todo el I/O real. Reportamos
    // progreso granular archivo a archivo a medida que se procesa cada uno.
    let tree = scanner::build_tree(
        &cfg.input,
        &ScanOptions {
            extensions: &cfg.extensions,
            dirs_only: cfg.dirs_only,
            excluded_paths: &excluded,
            excluded_dir_names: &cfg.exclude_dirs,
            counter: progress.as_ref().map(|p| &p.done),
            dirs_counter: progress.as_ref().map(|p| &p.dirs_scanned),
            cancel: cancel_flag,
            log: Some(&log),
        },
    )
    .context("Error al construir el árbol de directorios")?;
    if is_cancelled(cancel_flag) {
        bail!("Escaneo cancelado por el usuario");
    }

    // Totales del árbol (operación en memoria, sin I/O). Las filas del CSV
    // se generan al escribirlo, sin una lista intermedia.
    let (files_count, total_bytes) = scanner::file_totals(&tree);
    let dirs_count = scanner::count_dirs(&tree);
    let ext_stats = scanner::extension_stats(&tree);
    debug!(
        files_count,
        dirs_count,
        skipped = log.skipped_count(),
        elapsed = ?scan_start.elapsed(),
        "Escaneo terminado"
    );

    // Asegurar que el contador refleje la realidad final (en modo solo
    // carpetas el progreso cuenta carpetas, no archivos)
    if let Some(p) = &progress {
        let final_count = if cfg.dirs_only {
            dirs_count
        } else {
            files_count
        };
        p.done.store(final_count, Ordering::Relaxed);
    }

    // Fase 4: crear la carpeta de salida (después de escanear, para que no
    // aparezca a mitad de árbol) y generar reportes.
    if let Some(p) = &progress {
        p.writing.store(true, Ordering::Relaxed);
    }

    // Envuelta en un closure para poder resetear `writing` en un único
    // lugar tanto si la escritura termina bien como si falla a mitad de
    // camino (los `?` de más abajo, sin esto, saltarían directo al `return`
    // de la función y dejarían `writing` en `true` para siempre. Hoy es
    // inofensivo porque la GUI gatea el spinner con `status == Running`,
    // pero es una trampa latente para cualquier uso futuro de la bandera
    // que no tenga ese gate).
    let write_result: Result<()> = (|| {
        std::fs::create_dir_all(&cfg.output).with_context(|| {
            format!(
                "No se pudo crear el directorio de salida {}",
                cfg.output.display()
            )
        })?;

        for (format, path) in &reports {
            debug!("Escribiendo {}", path.display());
            match format {
                ReportFormat::Csv => reporters::csv::write_csv(&tree, path)?,
                ReportFormat::Md => reporters::markdown::write_markdown(&tree, &ext_stats, path)?,
                ReportFormat::Json => reporters::json::write_json(&tree, path)?,
            }
        }
        Ok(())
    })();

    if let Some(p) = &progress {
        p.writing.store(false, Ordering::Relaxed);
    }
    write_result?;

    let log = log.into_report();

    Ok(RunSummary {
        reports,
        files_count,
        total_bytes,
        dirs_count,
        dirs_only: cfg.dirs_only,
        skipped_count: log.skipped_count,
        skipped: log.skipped,
        excluded_dirs: log.excluded_dirs,
        ext_stats,
    })
}

/// `true` si se pidió cancelar el escaneo en curso.
fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_extensions_accepts_mixed_separators_dots_and_case() {
        assert_eq!(
            parse_extensions(["pdf, .DOCX;jpg  txt"]),
            vec!["pdf", "docx", "jpg", "txt"]
        );
    }

    #[test]
    fn parse_extensions_trims_values_split_by_the_cli() {
        // `-e "pdf, docx"` con `value_delimiter = ','` llega como ["pdf", " docx"].
        assert_eq!(parse_extensions(["pdf", " docx"]), vec!["pdf", "docx"]);
    }

    #[test]
    fn parse_extensions_drops_empty_and_duplicates() {
        assert_eq!(parse_extensions(["", " , ;", "txt", ".TXT"]), vec!["txt"]);
        assert!(parse_extensions(std::iter::empty()).is_empty());
    }

    #[test]
    fn run_counts_scanned_folders_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir_all(input.join("a").join("b")).unwrap();
        fs::write(input.join("a").join("x.txt"), "x").unwrap();

        let cfg = RunConfig {
            input,
            output: dir.path().join("out"),
            extensions: vec!["pdf".to_string()],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: vec![ReportFormat::Json],
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let progress = Progress::new();
        run(&cfg, Some(progress.clone())).unwrap();
        assert_eq!(progress.done.load(Ordering::Relaxed), 0);
        // La raíz, `a` y `b`.
        assert_eq!(progress.dirs_scanned.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn run_fails_on_nonexistent_input() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = RunConfig {
            input: dir.path().join("no_existe"),
            output: dir.path().join("out"),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: ReportFormat::ALL.to_vec(),
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let err = run(&cfg, None).unwrap_err();
        assert!(err.to_string().contains("no existe"));
    }

    #[test]
    fn run_fails_when_input_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("archivo.txt");
        fs::write(&file_path, "x").unwrap();
        let cfg = RunConfig {
            input: file_path,
            output: dir.path().join("out"),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: ReportFormat::ALL.to_vec(),
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let err = run(&cfg, None).unwrap_err();
        assert!(err.to_string().contains("no es un directorio"));
    }

    #[test]
    fn run_happy_path_generates_three_reports() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("a.txt"), "hello").unwrap();
        fs::write(input.join("b.txt"), "world").unwrap();

        let cfg = RunConfig {
            input,
            output: output.clone(),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: ReportFormat::ALL.to_vec(),
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let summary = run(&cfg, None).unwrap();

        assert_eq!(summary.files_count, 2);
        assert_eq!(summary.total_bytes, 10); // "hello" + "world"
        assert_eq!(summary.skipped_count, 0);
        assert_eq!(summary.reports.len(), 3);
        assert!(summary.reports.iter().all(|(_, p)| p.exists()));
        assert_eq!(
            summary.reports[0],
            (ReportFormat::Csv, output.join("report.csv"))
        );
    }

    #[test]
    fn parse_dir_names_splits_by_comma_only_and_keeps_case() {
        assert_eq!(
            parse_dir_names(["Mis fotos, .git;node_modules/", " .git ", "", "build\\"]),
            vec!["Mis fotos", ".git", "node_modules", "build"]
        );
    }

    #[test]
    fn run_omits_excluded_dirs_without_counting_them_as_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir_all(input.join("target").join("debug")).unwrap();
        fs::write(input.join("target").join("debug").join("app.exe"), "x").unwrap();
        fs::write(input.join("main.rs"), "x").unwrap();

        let cfg = RunConfig {
            input,
            output: dir.path().join("out"),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec!["target".to_string()],
            formats: vec![ReportFormat::Md],
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let summary = run(&cfg, None).unwrap();
        assert_eq!(summary.files_count, 1);
        assert_eq!(summary.dirs_count, 0);
        assert_eq!(summary.skipped_count, 0);
        assert!(summary.skipped.is_empty());
        assert_eq!(summary.excluded_dirs, vec![("target".to_string(), 1)]);
        let md = fs::read_to_string(dir.path().join("out").join("report.md")).unwrap();
        assert!(!md.contains("target"));
        // Las estadísticas solo cuentan lo reportado (sin el .exe omitido).
        assert_eq!(summary.ext_stats.len(), 1);
        assert_eq!(summary.ext_stats[0].extension, "rs");
        assert!(md.contains("| .rs | 1 | 1 B | 100.0 % |"));
    }

    #[test]
    fn run_ignores_leftover_temp_reports() {
        // Un `report.csv.tmp` que quedó de una corrida interrumpida no debe
        // aparecer en el nuevo reporte, y el nuevo `report.csv` lo reemplaza.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("a.txt"), "x").unwrap();
        fs::write(input.join("report.csv.tmp"), "sobra").unwrap();

        let cfg = RunConfig {
            input: input.clone(),
            output: input.clone(),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: vec![ReportFormat::Csv],
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let summary = run(&cfg, None).unwrap();
        assert_eq!(summary.files_count, 1);
        assert!(!input.join("report.csv.tmp").exists());
        let csv = fs::read_to_string(input.join("report.csv")).unwrap();
        assert!(!csv.contains("report.csv"));
    }

    #[test]
    fn report_name_changes_the_file_names() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("a.txt"), "x").unwrap();
        // Reportes de corridas anteriores en la misma carpeta: con el
        // nombre por defecto y con nombre automático. No deben aparecer.
        fs::write(input.join("report.csv"), "viejo").unwrap();
        fs::write(input.join("report-in-2026-11-12.json"), "viejo").unwrap();

        let cfg = RunConfig {
            input: input.clone(),
            output: input.clone(),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: vec![ReportFormat::Csv, ReportFormat::Md],
            report_name: "facturas".to_string(),
        };
        let summary = run(&cfg, None).unwrap();
        assert_eq!(summary.files_count, 1);
        assert_eq!(summary.reports[0].1, input.join("facturas.csv"));
        assert!(input.join("facturas.md").is_file());
        // El reporte anterior con el nombre por defecto no se toca.
        assert_eq!(
            fs::read_to_string(input.join("report.csv")).unwrap(),
            "viejo"
        );
    }

    #[test]
    fn run_rejects_invalid_report_names() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = RunConfig {
            input: dir.path().to_path_buf(),
            output: dir.path().join("out"),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: vec![ReportFormat::Csv],
            report_name: "sub/facturas".to_string(),
        };
        let err = run(&cfg, None).unwrap_err();
        assert!(err.to_string().contains("no puede contener «/»"), "{err}");
        assert!(!dir.path().join("out").exists());
    }

    #[test]
    fn validate_report_name_accepts_normal_names_only() {
        for ok in ["report", "facturas 2026", "inventario.v2", "año-ñandú"] {
            assert!(validate_report_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "  ", "a/b", r"a\b", "a:b", "a*", "a?", "a\"b", "<a>", "a|b", "a.", "a ",
        ] {
            assert!(validate_report_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn auto_report_name_uses_folder_and_date_and_skips_taken_names() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path();
        let date = chrono::NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let input = Path::new("D:/datos/Mis docs");
        let formats = [ReportFormat::Csv, ReportFormat::Json];

        assert_eq!(
            auto_report_name(input, out, &formats, date),
            "report-Mis docs-2026-11-12"
        );
        // Basta con que exista uno de los formatos pedidos para pasar al
        // siguiente número.
        fs::write(out.join("report-Mis docs-2026-11-12.json"), "").unwrap();
        assert_eq!(
            auto_report_name(input, out, &formats, date),
            "report-Mis docs-2026-11-12-2"
        );
        fs::write(out.join("report-Mis docs-2026-11-12-2.csv"), "").unwrap();
        assert_eq!(
            auto_report_name(input, out, &formats, date),
            "report-Mis docs-2026-11-12-3"
        );
        // Un formato no pedido no ocupa el nombre.
        assert_eq!(
            auto_report_name(input, out, &[ReportFormat::Md], date),
            "report-Mis docs-2026-11-12"
        );
    }

    #[test]
    fn auto_report_name_handles_drive_roots() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let name = auto_report_name(Path::new(r"C:\"), dir.path(), &ReportFormat::ALL, date);
        assert!(validate_report_name(&name).is_ok(), "{name}");
        assert!(name.ends_with("-2026-01-02"), "{name}");
    }

    #[test]
    fn is_auto_report_name_matches_only_generated_names() {
        for yes in [
            "report-docs-2026-11-12.csv",
            "report-docs-2026-11-12-2.md",
            "report-mis-docs-2026-11-12.json",
            "report-docs-2026-11-12.csv.tmp",
        ] {
            assert!(is_auto_report_name(yes), "{yes}");
        }
        for no in [
            "report.csv",
            "report-final.csv",
            "report-2026-11-12.csv",
            "report-docs-2026-11-12.txt",
            "report-docs-2026-13-40.csv",
            "reporte-docs-2026-11-12.csv",
            "report-docs-2026-11-12-.csv",
        ] {
            assert!(!is_auto_report_name(no), "{no}");
        }
    }

    #[test]
    fn run_generates_only_the_requested_formats() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("a.txt"), "hello").unwrap();

        let cfg = RunConfig {
            input,
            output: output.clone(),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            // Desordenado y con repetidos: se normaliza al orden de ALL.
            formats: vec![ReportFormat::Json, ReportFormat::Csv, ReportFormat::Json],
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let summary = run(&cfg, None).unwrap();

        let formats: Vec<ReportFormat> = summary.reports.iter().map(|(f, _)| *f).collect();
        assert_eq!(formats, vec![ReportFormat::Csv, ReportFormat::Json]);
        assert!(output.join("report.csv").exists());
        assert!(output.join("report.json").exists());
        assert!(!output.join("report.md").exists());
    }

    #[test]
    fn run_fails_without_formats() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = RunConfig {
            input: dir.path().to_path_buf(),
            output: dir.path().join("out"),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: vec![],
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let err = run(&cfg, None).unwrap_err();
        assert!(err.to_string().contains("formato"));
        assert!(!dir.path().join("out").exists());
    }

    #[test]
    fn run_dirs_only_omits_files_in_summary() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        let output = dir.path().join("out");
        fs::create_dir(&input).unwrap();
        fs::create_dir(input.join("subfolder")).unwrap();
        fs::write(input.join("a.txt"), "hello").unwrap();
        fs::write(input.join("subfolder").join("b.txt"), "world").unwrap();

        let cfg = RunConfig {
            input,
            output,
            extensions: vec![],
            dirs_only: true,
            exclude_dirs: vec![],
            formats: ReportFormat::ALL.to_vec(),
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let summary = run(&cfg, None).unwrap();

        assert_eq!(summary.files_count, 0);
        assert_eq!(summary.dirs_count, 1);
        assert!(summary.dirs_only);
        assert_eq!(summary.reports[1].0, ReportFormat::Md);
        let md_content = fs::read_to_string(&summary.reports[1].1).unwrap();
        assert!(md_content.contains("subfolder"));
        assert!(!md_content.contains("a.txt"));
        assert!(!md_content.contains("b.txt"));
    }

    #[test]
    fn run_clears_writing_flag_when_done() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("a.txt"), "hello").unwrap();

        let cfg = RunConfig {
            input,
            output: dir.path().join("out"),
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: ReportFormat::ALL.to_vec(),
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let progress = Progress::new();
        run(&cfg, Some(progress.clone())).unwrap();

        // Regresión: si `writing` se queda en `true` tras terminar, la GUI
        // muestra un spinner "Generando reportes…" permanente.
        assert!(!progress.writing.load(Ordering::Relaxed));
    }

    #[test]
    fn run_clears_writing_flag_when_write_phase_fails() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir(&input).unwrap();
        fs::write(input.join("a.txt"), "hello").unwrap();

        // La "carpeta" de salida es en realidad un archivo ya existente:
        // `create_dir_all` (el primer paso de la fase de escritura) falla,
        // forzando el camino de error dentro de la fase 4.
        let output = dir.path().join("blocked");
        fs::write(&output, "soy un archivo, no una carpeta").unwrap();

        let cfg = RunConfig {
            input,
            output,
            extensions: vec![],
            dirs_only: false,
            exclude_dirs: vec![],
            formats: ReportFormat::ALL.to_vec(),
            report_name: DEFAULT_REPORT_NAME.to_string(),
        };
        let progress = Progress::new();
        let result = run(&cfg, Some(progress.clone()));

        assert!(result.is_err());
        // Regresión: antes del fix, un error durante la escritura de
        // reportes propagaba con `?` sin pasar por el reset de `writing`,
        // dejándolo en `true` para siempre.
        assert!(!progress.writing.load(Ordering::Relaxed));
    }
}
