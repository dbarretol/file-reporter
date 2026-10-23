use crate::models::FileRecord;
use anyhow::{Context, Result};
use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// BOM UTF-8: marca al inicio del archivo que indica a Excel
/// (y otros programas) que el contenido está codificado en UTF-8.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Antepone un apóstrofo a los valores que Excel interpretaría como fórmula
/// (empiezan por `=`, `+`, `-`, `@`, tab o CR) para neutralizar CSV
/// injection: un archivo llamado, p. ej., `=cmd|'/c calc'!A1.txt` no debe
/// ejecutarse al abrir el reporte. El apóstrofo hace que Excel muestre el
/// valor como texto literal en vez de evaluarlo.
fn sanitize_for_spreadsheet(s: &str) -> Cow<'_, str> {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        Cow::Owned(format!("'{s}"))
    } else {
        Cow::Borrowed(s)
    }
}

pub fn write_csv(records: &[FileRecord], output: &Path) -> Result<()> {
    // 1. Crear el archivo manualmente para poder escribir el BOM primero.
    let file = File::create(output)
        .with_context(|| format!("No se pudo crear CSV en {}", output.display()))?;
    let mut buf_writer = BufWriter::new(file);

    // 2. Escribir BOM UTF-8 → fuerza a Excel a interpretar el archivo como UTF-8.
    buf_writer
        .write_all(UTF8_BOM)
        .context("No se pudo escribir el BOM UTF-8 en el CSV")?;

    // 3. Construir el CSV writer con punto y coma (;) como delimitador,
    //    compatible con la configuración regional de Excel en es-ES / es-LA.
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b';')
        .quote_style(csv::QuoteStyle::Necessary) // Solo entrecomilla cuando hace falta
        .from_writer(buf_writer);

    // 4. Serializar todos los registros (incluye el header automáticamente).
    //    `name`/`directory`/`full_path` provienen de nombres de archivo en
    //    disco, fuera del control del usuario (descargas, carpetas
    //    compartidas): se sanean antes de escribir para evitar CSV
    //    injection. `extension`/fechas/tamaños no son texto libre.
    for r in records {
        let sanitized = FileRecord {
            name: sanitize_for_spreadsheet(&r.name).into_owned(),
            directory: sanitize_for_spreadsheet(&r.directory).into_owned(),
            full_path: sanitize_for_spreadsheet(&r.full_path).into_owned(),
            ..r.clone()
        };
        wtr.serialize(&sanitized)?;
    }

    wtr.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_for_spreadsheet_prefixes_formula_triggers() {
        for dangerous in ["=1+1", "+1", "-informe.txt", "@SUM(A1)", "\ttab", "\rcr"] {
            let sanitized = sanitize_for_spreadsheet(dangerous);
            assert_eq!(sanitized, format!("'{dangerous}"));
        }
    }

    #[test]
    fn sanitize_for_spreadsheet_leaves_normal_text_untouched() {
        assert_eq!(sanitize_for_spreadsheet("informe.txt"), "informe.txt");
        assert_eq!(sanitize_for_spreadsheet(""), "");
    }

    #[test]
    fn write_csv_neutralizes_formula_injection_in_name() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("report.csv");

        let record = FileRecord {
            name: "=cmd|'/c calc'!A1".to_string(),
            extension: "txt".to_string(),
            size_mb: 0.0,
            size_bytes: 0,
            created: String::new(),
            modified: String::new(),
            created_iso: String::new(),
            modified_iso: String::new(),
            directory: "-informe".to_string(),
            full_path: "@evil".to_string(),
        };
        write_csv(&[record], &output).unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        // Regresión: sin el fix, Excel evaluaría estos campos como fórmulas
        // al abrir report.csv (CSV injection, ver tarea 37).
        assert!(content.contains("'=cmd|'/c calc'!A1"));
        assert!(content.contains("'-informe"));
        assert!(content.contains("'@evil"));
    }
}
