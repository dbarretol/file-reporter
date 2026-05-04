use crate::{reporters, scanner};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Parámetros de ejecución (válidos tanto para CLI como GUI).
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub extensions: Vec<String>,
}

/// Reporte de progreso compartido entre el worker y la GUI.
#[derive(Debug, Default)]
pub struct Progress {
    pub total: AtomicUsize, // archivos detectados (estimado)
    pub done: AtomicUsize,  // archivos procesados
    pub finished: AtomicBool,
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        let done = self.done.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32).min(1.0)
        }
    }
}

/// Resultado de una ejecución exitosa.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub csv_path: PathBuf,
    pub md_path: PathBuf,
    pub json_path: PathBuf,
    pub files_count: usize,
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

    std::fs::create_dir_all(&cfg.output).with_context(|| {
        format!(
            "No se pudo crear el directorio de salida {}",
            cfg.output.display()
        )
    })?;

    // Fase 1: contar archivos para estimar el progreso (rápido)
    if let Some(p) = &progress {
        let total = count_files(&cfg.input, &cfg.extensions);
        p.total.store(total, Ordering::Relaxed);
        p.done.store(0, Ordering::Relaxed);
    }

    // Fase 2: construir árbol
    let tree = scanner::build_tree(&cfg.input, &cfg.extensions)
        .context("Error al construir el árbol de directorios")?;

    // Fase 3: aplanar a registros para CSV (con progreso granular archivo a archivo)
    let mut records = Vec::new();
    scanner::flatten_to_records(&tree, &mut records, progress.as_ref().map(|p| &p.done));

    // Asegurar que el contador refleje la realidad final
    if let Some(p) = &progress {
        let final_count = records.len();
        p.done.store(final_count, Ordering::Relaxed);
        if p.total.load(Ordering::Relaxed) < final_count {
            p.total.store(final_count, Ordering::Relaxed);
        }
    }

    // Fase 4: generar reportes
    let csv_path = cfg.output.join("report.csv");
    let md_path = cfg.output.join("report.md");
    let json_path = cfg.output.join("report.json");

    reporters::csv::write_csv(&records, &csv_path)?;
    reporters::markdown::write_markdown(&tree, &md_path)?;
    reporters::json::write_json(&tree, &json_path)?;

    if let Some(p) = &progress {
        p.finished.store(true, Ordering::Relaxed);
    }

    Ok(RunSummary {
        csv_path,
        md_path,
        json_path,
        files_count: records.len(),
    })
}

/// Cuenta archivos rápidamente (sin leer metadatos completos) para el ETA.
fn count_files(path: &Path, filters: &[String]) -> usize {
    let mut count = 0;
    if let Ok(rd) = std::fs::read_dir(path) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                count += count_files(&p, filters);
            } else if filters.is_empty() {
                count += 1;
            } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if filters.iter().any(|f| f.eq_ignore_ascii_case(ext)) {
                    count += 1;
                }
            }
        }
    }
    count
}
