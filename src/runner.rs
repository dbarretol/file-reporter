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
}

/// Reporte de progreso compartido entre el worker y la GUI.
#[derive(Debug, Default)]
pub struct Progress {
    pub total: AtomicUsize,   // archivos detectados (estimado)
    pub done: AtomicUsize,    // archivos procesados
    pub cancel: AtomicBool,   // la GUI la pone en true para pedir cancelación
    pub skipped: AtomicUsize, // carpetas/archivos que no se pudieron leer
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

    /// Devuelve la fracción de progreso en [0.0, 1.0].
    /// Protegido contra NaN, infinitos y valores fuera de rango.
    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        let done = self.done.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let f = done as f32 / total as f32;
        if f.is_finite() {
            f.clamp(0.0, 1.0)
        } else {
            0.0
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
    // árbol), casi duplicando el tiempo total. El `total` de `Progress`
    // queda en 0 ("desconocido") durante todo el escaneo; la GUI lo trata
    // como progreso indeterminado y muestra el contador `done` en vivo. Al
    // terminar, más abajo, `total` se fija al conteo final real.
    if let Some(p) = &progress {
        p.done.store(0, Ordering::Relaxed);
        p.total.store(0, Ordering::Relaxed);
    }

    // Única pasada por el disco: aquí ocurre todo el I/O real. Reportamos
    // progreso granular archivo a archivo a medida que se procesa cada uno.
    let tree = scanner::build_tree(
        &cfg.input,
        &cfg.extensions,
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

    // Asegurar que el contador refleje la realidad final
    if let Some(p) = &progress {
        let final_count = records.len();
        p.done.store(final_count, Ordering::Relaxed);
        if p.total.load(Ordering::Relaxed) < final_count {
            p.total.store(final_count, Ordering::Relaxed);
        }
    }

    // Fase 4: crear la carpeta de salida (después de escanear, para que no
    // aparezca a mitad de árbol) y generar reportes.
    if let Some(p) = &progress {
        p.writing.store(true, Ordering::Relaxed);
    }
    std::fs::create_dir_all(&cfg.output).with_context(|| {
        format!(
            "No se pudo crear el directorio de salida {}",
            cfg.output.display()
        )
    })?;

    reporters::csv::write_csv(&records, &csv_path)?;
    reporters::markdown::write_markdown(&tree, &md_path)?;
    reporters::json::write_json(&tree, &json_path)?;

    let skipped_count = skipped.load(Ordering::Relaxed);
    if let Some(p) = &progress {
        p.skipped.store(skipped_count, Ordering::Relaxed);
        // La fase de escritura terminó: sin esto, `writing` queda en `true`
        // para siempre y la GUI cree que sigue generando reportes.
        p.writing.store(false, Ordering::Relaxed);
    }

    Ok(RunSummary {
        csv_path,
        md_path,
        json_path,
        files_count: records.len(),
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
    fn run_fails_on_nonexistent_input() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = RunConfig {
            input: dir.path().join("no_existe"),
            output: dir.path().join("out"),
            extensions: vec![],
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
        };
        let summary = run(&cfg, None).unwrap();

        assert_eq!(summary.files_count, 2);
        assert_eq!(summary.skipped_count, 0);
        assert!(summary.csv_path.exists());
        assert!(summary.md_path.exists());
        assert!(summary.json_path.exists());
        assert_eq!(summary.csv_path, output.join("report.csv"));
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
        };
        let progress = Progress::new();
        run(&cfg, Some(progress.clone())).unwrap();

        // Regresión: si `writing` se queda en `true` tras terminar, la GUI
        // muestra un spinner "Generando reportes…" permanente (ver tarea 26).
        assert!(!progress.writing.load(Ordering::Relaxed));
    }
}


