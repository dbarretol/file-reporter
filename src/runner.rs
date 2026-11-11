use crate::{reporters, scanner};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Parámetros de ejecución (válidos tanto para CLI como GUI).
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub extensions: Vec<String>,
    pub dirs_only: bool,
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

/// Reporte de progreso compartido entre el worker y la GUI.
#[derive(Debug, Default)]
pub struct Progress {
    /// Archivos (o carpetas, en modo solo carpetas) encontrados hasta ahora.
    pub done: AtomicUsize,
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
    pub csv_path: PathBuf,
    pub md_path: PathBuf,
    pub json_path: PathBuf,
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
}

/// Ejecuta el proceso completo: escanear + generar 3 reportes.
/// Reporta progreso si se pasa un `Progress`.
pub fn run(cfg: &RunConfig, progress: Option<Arc<Progress>>) -> Result<RunSummary> {
    if !cfg.input.exists() {
        bail!("La ruta no existe: {}", cfg.input.display());
    }
    if !cfg.input.is_dir() {
        bail!("La ruta no es un directorio: {}", cfg.input.display());
    }

    let csv_path = cfg.output.join("report.csv");
    let md_path = cfg.output.join("report.md");
    let json_path = cfg.output.join("report.json");

    // Excluir del escaneo los reportes de una corrida anterior (siempre) y,
    // si la salida es una carpeta distinta de la entrada, la carpeta de
    // salida completa (por si contiene sobras de corridas previas). Cuando
    // output == input no se puede excluir la carpeta entera, solo los 3
    // archivos exactos.
    let mut excluded = vec![csv_path.clone(), md_path.clone(), json_path.clone()];
    if cfg.output != cfg.input {
        excluded.push(cfg.output.clone());
    }

    let cancel_flag = progress.as_ref().map(|p| &p.cancel);
    let skipped = AtomicUsize::new(0);

    // No hacemos una pasada previa solo para contar archivos: eso implicaría
    // leer el disco dos veces (una para contar, otra para construir el
    // árbol), casi duplicando el tiempo total. El total se desconoce hasta
    // el final; la GUI muestra el contador `done` en vivo y el tiempo
    // transcurrido.
    if let Some(p) = &progress {
        p.done.store(0, Ordering::Relaxed);
    }

    // Única pasada por el disco: aquí ocurre todo el I/O real. Reportamos
    // progreso granular archivo a archivo a medida que se procesa cada uno.
    let tree = scanner::build_tree(
        &cfg.input,
        &cfg.extensions,
        cfg.dirs_only,
        progress.as_ref().map(|p| &p.done),
        &excluded,
        cancel_flag,
        Some(&skipped),
    )
    .context("Error al construir el árbol de directorios")?;
    if is_cancelled(cancel_flag) {
        bail!("Escaneo cancelado por el usuario");
    }

    // Fase 3: aplanar a registros para CSV (operación en memoria, sin I/O)
    let mut records = Vec::new();
    scanner::flatten_to_records(&tree, &mut records);
    let dirs_count = scanner::count_dirs(&tree);

    // Asegurar que el contador refleje la realidad final (en modo solo
    // carpetas el progreso cuenta carpetas, no archivos)
    if let Some(p) = &progress {
        let final_count = if cfg.dirs_only {
            dirs_count
        } else {
            records.len()
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

        reporters::csv::write_csv(&records, &csv_path)?;
        reporters::markdown::write_markdown(&tree, &md_path)?;
        reporters::json::write_json(&tree, &json_path)?;
        Ok(())
    })();

    if let Some(p) = &progress {
        p.writing.store(false, Ordering::Relaxed);
    }
    write_result?;

    let skipped_count = skipped.load(Ordering::Relaxed);

    Ok(RunSummary {
        csv_path,
        md_path,
        json_path,
        files_count: records.len(),
        total_bytes: records.iter().map(|r| r.size_bytes).sum(),
        dirs_count,
        dirs_only: cfg.dirs_only,
        skipped_count,
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
    fn run_fails_on_nonexistent_input() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = RunConfig {
            input: dir.path().join("no_existe"),
            output: dir.path().join("out"),
            extensions: vec![],
            dirs_only: false,
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
        };
        let summary = run(&cfg, None).unwrap();

        assert_eq!(summary.files_count, 2);
        assert_eq!(summary.total_bytes, 10); // "hello" + "world"
        assert_eq!(summary.skipped_count, 0);
        assert!(summary.csv_path.exists());
        assert!(summary.md_path.exists());
        assert!(summary.json_path.exists());
        assert_eq!(summary.csv_path, output.join("report.csv"));
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
        };
        let summary = run(&cfg, None).unwrap();

        assert_eq!(summary.files_count, 0);
        assert_eq!(summary.dirs_count, 1);
        assert!(summary.dirs_only);
        assert!(summary.md_path.exists());
        let md_content = fs::read_to_string(&summary.md_path).unwrap();
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
