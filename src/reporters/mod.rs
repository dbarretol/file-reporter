pub mod csv;
pub mod json;
pub mod markdown;

use anyhow::{Context, Result};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Archivo temporal en el que se escribe un reporte antes de colocarlo en
/// su sitio: `report.csv` → `report.csv.tmp`, en la misma carpeta (para que
/// el renombrado final no cruce de unidad).
pub fn temp_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    output.with_file_name(name)
}

/// Escribe `output` sin arriesgar la versión anterior: `write` vuelca el
/// contenido en `temp_path(output)` y, solo si todo fue bien, el temporal
/// reemplaza a `output` con un renombrado. Si algo falla (disco lleno,
/// `output` abierto y bloqueado por otro programa…), el temporal se borra
/// y el reporte anterior queda intacto, en vez de vacío o a medias como
/// con `File::create`, que lo trunca antes de empezar. `label` nombra el
/// formato en los mensajes de error ("CSV", "Markdown"…).
pub fn write_atomic(
    output: &Path,
    label: &str,
    write: impl FnOnce(&mut BufWriter<File>) -> Result<()>,
) -> Result<()> {
    let tmp = temp_path(output);
    let result = (|| {
        let file = File::create(&tmp)
            .with_context(|| format!("No se pudo crear {label} en {}", tmp.display()))?;
        let mut writer = BufWriter::new(file);
        write(&mut writer)?;
        writer
            .flush()
            .with_context(|| format!("No se pudo escribir {label} en {}", tmp.display()))?;
        drop(writer);
        // `fs::rename` reemplaza el destino si existe (también en Windows).
        fs::rename(&tmp, output).with_context(|| {
            format!(
                "No se pudo reemplazar {} (¿está abierto en otro programa?)",
                output.display()
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_path_appends_tmp_in_the_same_folder() {
        let out = Path::new("salida").join("report.csv");
        assert_eq!(temp_path(&out), Path::new("salida").join("report.csv.tmp"));
    }

    #[test]
    fn write_atomic_replaces_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("report.csv");
        fs::write(&out, "viejo").unwrap();

        write_atomic(&out, "CSV", |w| Ok(w.write_all(b"nuevo")?)).unwrap();

        assert_eq!(fs::read_to_string(&out).unwrap(), "nuevo");
        assert!(!temp_path(&out).exists());
    }

    #[test]
    fn write_atomic_keeps_the_previous_file_if_writing_fails() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("report.csv");
        fs::write(&out, "reporte anterior").unwrap();

        // Falla a mitad de la escritura, después de haber escrito algo.
        let err = write_atomic(&out, "CSV", |w| {
            w.write_all(b"medio reporte")?;
            anyhow::bail!("disco lleno")
        })
        .unwrap_err();

        assert!(err.to_string().contains("disco lleno"));
        assert_eq!(fs::read_to_string(&out).unwrap(), "reporte anterior");
        assert!(!temp_path(&out).exists(), "el temporal se borra");
    }

    #[test]
    fn write_atomic_cleans_up_if_the_file_cannot_be_replaced() {
        // El destino es una carpeta: el renombrado final falla.
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("report.csv");
        fs::create_dir(&out).unwrap();

        let err = write_atomic(&out, "CSV", |w| Ok(w.write_all(b"x")?)).unwrap_err();

        assert!(err.to_string().contains("No se pudo reemplazar"));
        assert!(out.is_dir());
        assert!(!temp_path(&out).exists());
    }
}
