use crate::models::{FileRecord, Node};
use crate::scanner;
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

/// Escribe una fila por archivo del árbol, generándolas de una en una
/// mientras se recorre (sin una lista intermedia en memoria).
pub fn write_csv(root: &Node, output: &Path) -> Result<()> {
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
    //    `name`/`extension`/`directory`/`full_path` provienen de nombres de
    //    archivo en disco, fuera del control del usuario (descargas, carpetas
    //    compartidas): se sanean antes de escribir para evitar CSV
    //    injection. La extensión también sale del nombre (p. ej. `x.=1+1`
    //    tiene extensión `=1+1`). Fechas/tamaños no son texto libre.
    scanner::try_for_each_record(root, &mut |r| {
        let sanitized = FileRecord {
            name: sanitize_for_spreadsheet(&r.name).into_owned(),
            extension: sanitize_for_spreadsheet(&r.extension).into_owned(),
            directory: sanitize_for_spreadsheet(&r.directory).into_owned(),
            full_path: sanitize_for_spreadsheet(&r.full_path).into_owned(),
            ..r
        };
        wtr.serialize(&sanitized)
            .with_context(|| format!("No se pudo escribir el CSV en {}", output.display()))
    })?;

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
        // Nombres reales en disco (válidos también en Windows) que Excel
        // evaluaría como fórmula al abrir report.csv (CSV injection).
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        std::fs::create_dir(&input).unwrap();
        for name in ["=1+1.txt", "-informe.txt", "@evil.txt", "x.=1+1"] {
            std::fs::write(input.join(name), "x").unwrap();
        }
        let tree = scanner::build_tree(&input, &scanner::ScanOptions::default()).unwrap();

        let output = dir.path().join("report.csv");
        write_csv(&tree, &output).unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.starts_with('\u{feff}'), "falta el BOM UTF-8");
        assert_eq!(content.lines().count(), 5, "cabecera + 4 filas");
        // `name` es la primera columna: cada fila empieza por el nombre saneado.
        assert!(content.contains("\n'=1+1;"));
        assert!(content.contains("\n'-informe;"));
        assert!(content.contains("\n'@evil;"));
        // La extensión también sale del nombre: en `x.=1+1` Excel evaluaría
        // `=1+1` como fórmula en la segunda columna.
        assert!(content.contains("\nx;'=1+1;"));
    }
}
