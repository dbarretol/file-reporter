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

    /// Nombre fijo del archivo de este formato en la carpeta de salida.
    pub fn file_name(self) -> &'static str {
        match self {
            ReportFormat::Csv => "report.csv",
            ReportFormat::Md => "report.md",
            ReportFormat::Json => "report.json",
        }
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
}

impl RunConfig {
    /// Rutas de los reportes que generaría esta corrida, en el orden de
    /// `ReportFormat::ALL` y sin repetidos, se hayan pedido como se hayan
    /// pedido (p. ej. `-f json,csv,json`).
    pub fn report_paths(&self) -> Vec<(ReportFormat, PathBuf)> {
        ReportFormat::ALL
            .into_iter()
            .filter(|f| self.formats.contains(f))
            .map(|f| (f, self.output.join(f.file_name())))
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
/// sugerencia "Comunes" de la GUI.
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

/// Tamaño legible para el usuario, en unidades binarias (1 KB = 1024 B):
/// `512 B`, `1.5 KB`, `2.3 GB`.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Porcentaje legible con un decimal (`25.0 %`); `<0.1 %` para partes no
/// nulas muy pequeñas, que si no se mostrarían como `0.0 %`.
pub fn format_percent(part: u64, total: u64) -> String {
    if total == 0 {
        return "0 %".to_string();
    }
    let p = part as f64 * 100.0 / total as f64;
    if part > 0 && p < 0.1 {
        "<0.1 %".to_string()
    } else {
        format!("{p:.1} %")
    }
}

/// Fila del resumen de tipos de archivo que muestran la CLI y la GUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRow {
    pub label: String,
    pub count: usize,
    pub bytes: u64,
}

/// Los `n` tipos con más tamaño y, si quedan más, una fila "Otros (k tipos)"
/// que los suma. Si solo sobraría uno, se muestra tal cual en vez de
/// agruparlo. `stats` debe venir ordenado (como `RunSummary::ext_stats`).
pub fn top_types(stats: &[ExtStat], n: usize) -> Vec<TypeRow> {
    let row = |s: &ExtStat| TypeRow {
        label: s.label(),
        count: s.count,
        bytes: s.bytes,
    };
    if stats.len() <= n + 1 {
        return stats.iter().map(row).collect();
    }
    let (top, rest) = stats.split_at(n);
    let mut rows: Vec<TypeRow> = top.iter().map(row).collect();
    rows.push(TypeRow {
        label: format!("Otros ({} tipos)", rest.len()),
        count: rest.iter().map(|s| s.count).sum(),
        bytes: rest.iter().map(|s| s.bytes).sum(),
    });
    rows
}

/// Qué cuenta `Progress::done`, para los textos de progreso de la CLI y la GUI.
pub fn found_noun(dirs_only: bool) -> &'static str {
    if dirs_only {
        "carpetas encontradas"
    } else {
        "archivos encontrados"
    }
}

/// Estado del escaneo en curso para la CLI y la GUI: lo encontrado y, salvo
/// en modo solo carpetas (donde ya se cuentan carpetas), las carpetas
/// revisadas. Con un filtro de extensiones lo encontrado puede quedarse en 0
/// mucho tiempo; las carpetas revisadas muestran que el escaneo avanza.
pub fn scan_status(found: usize, dirs_scanned: usize, dirs_only: bool) -> String {
    let noun = found_noun(dirs_only);
    if dirs_only {
        format!("{found} {noun}")
    } else {
        format!("{found} {noun} · {dirs_scanned} carpetas revisadas")
    }
}

/// Aviso de las carpetas omitidas por nombre, con los nombres que
/// coincidieron: "3 carpetas omitidas por nombre (.git, build ×2)". `None`
/// si no se omitió ninguna.
pub fn excluded_dirs_text(excluded: &[(String, usize)]) -> Option<String> {
    let total: usize = excluded.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return None;
    }
    let names: Vec<String> = excluded
        .iter()
        .map(|(name, n)| {
            if *n > 1 {
                format!("{name} ×{n}")
            } else {
                name.clone()
            }
        })
        .collect();
    let noun = if total == 1 {
        "carpeta omitida"
    } else {
        "carpetas omitidas"
    };
    Some(format!("{total} {noun} por nombre ({})", names.join(", ")))
}

/// Aviso de los elementos que no se pudieron leer.
pub fn skipped_text(count: usize) -> String {
    if count == 1 {
        "1 elemento no accesible fue omitido".to_string()
    } else {
        format!("{count} elementos no accesibles fueron omitidos")
    }
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

impl RunSummary {
    /// Ruta del reporte de `format`, si se generó.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn path(&self, format: ReportFormat) -> Option<&Path> {
        self.reports
            .iter()
            .find(|(f, _)| *f == format)
            .map(|(_, p)| p.as_path())
    }
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

    let reports = cfg.report_paths();

    // Excluir del escaneo los reportes de una corrida anterior (siempre, de
    // los tres formatos: también los que esta corrida no genera) y, si la
    // salida es una carpeta distinta de la entrada, la carpeta de salida
    // completa (por si contiene sobras de corridas previas). Cuando
    // output == input no se puede excluir la carpeta entera, solo los
    // archivos exactos.
    let mut excluded: Vec<PathBuf> = ReportFormat::ALL
        .iter()
        .map(|f| cfg.output.join(f.file_name()))
        .collect();
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
    fn format_size_picks_a_readable_unit() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(2_469_606_195), "2.3 GB");
    }

    #[test]
    fn format_percent_handles_zero_and_tiny_values() {
        assert_eq!(format_percent(0, 0), "0 %");
        assert_eq!(format_percent(1, 4), "25.0 %");
        assert_eq!(format_percent(1, 10_000), "<0.1 %");
        assert_eq!(format_percent(0, 10), "0.0 %");
    }

    fn stat(ext: &str, count: usize, bytes: u64) -> ExtStat {
        ExtStat {
            extension: ext.to_string(),
            count,
            bytes,
        }
    }

    #[test]
    fn top_types_groups_the_rest_under_otros() {
        let stats = [
            stat("pdf", 1, 100),
            stat("jpg", 2, 50),
            stat("txt", 3, 20),
            stat("", 4, 10),
        ];
        let rows = top_types(&stats, 2);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].label, ".pdf");
        assert_eq!(
            rows[2],
            TypeRow {
                label: "Otros (2 tipos)".to_string(),
                count: 7,
                bytes: 30,
            }
        );
    }

    #[test]
    fn top_types_does_not_group_a_single_leftover() {
        let stats = [stat("pdf", 1, 100), stat("jpg", 2, 50), stat("txt", 3, 20)];
        let labels: Vec<String> = top_types(&stats, 2).into_iter().map(|r| r.label).collect();
        assert_eq!(labels, vec![".pdf", ".jpg", ".txt"]);
        assert!(top_types(&[], 5).is_empty());
    }

    #[test]
    fn scan_status_adds_scanned_folders_unless_counting_folders() {
        assert_eq!(
            scan_status(120, 3400, false),
            "120 archivos encontrados · 3400 carpetas revisadas"
        );
        assert_eq!(scan_status(7, 8, true), "7 carpetas encontradas");
    }

    #[test]
    fn excluded_dirs_text_lists_names_and_repeats() {
        assert_eq!(excluded_dirs_text(&[]), None);
        assert_eq!(
            excluded_dirs_text(&[("build".to_string(), 1)]).as_deref(),
            Some("1 carpeta omitida por nombre (build)")
        );
        assert_eq!(
            excluded_dirs_text(&[(".git".to_string(), 1), ("build".to_string(), 2)]).as_deref(),
            Some("3 carpetas omitidas por nombre (.git, build ×2)")
        );
    }

    #[test]
    fn skipped_text_agrees_in_number() {
        assert_eq!(skipped_text(1), "1 elemento no accesible fue omitido");
        assert_eq!(skipped_text(4), "4 elementos no accesibles fueron omitidos");
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
        };
        let summary = run(&cfg, None).unwrap();

        assert_eq!(summary.files_count, 2);
        assert_eq!(summary.total_bytes, 10); // "hello" + "world"
        assert_eq!(summary.skipped_count, 0);
        assert_eq!(summary.reports.len(), 3);
        assert!(summary.reports.iter().all(|(_, p)| p.exists()));
        assert_eq!(
            summary.path(ReportFormat::Csv),
            Some(output.join("report.csv").as_path())
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
        };
        let summary = run(&cfg, None).unwrap();

        assert_eq!(summary.files_count, 0);
        assert_eq!(summary.dirs_count, 1);
        assert!(summary.dirs_only);
        let md_path = summary.path(ReportFormat::Md).unwrap();
        let md_content = fs::read_to_string(md_path).unwrap();
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
