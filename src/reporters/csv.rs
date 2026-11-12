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

/// Columnas del CSV, en el orden de los campos de `FileRecord` (un test
/// comprueba que coinciden con lo que serializa serde).
const HEADERS: [&str; 10] = [
    "name",
    "extension",
    "size_mb",
    "size_bytes",
    "created",
    "modified",
    "created_iso",
    "modified_iso",
    "directory",
    "full_path",
];

/// Formato común del CSV: `;` como separador (configuración regional de
/// Excel en español) y comillas solo cuando hacen falta.
fn writer_builder() -> csv::WriterBuilder {
    let mut builder = csv::WriterBuilder::new();
    builder
        .delimiter(b';')
        .quote_style(csv::QuoteStyle::Necessary);
    builder
}

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
    let mut wtr = writer_builder().has_headers(false).from_writer(buf_writer);

    // 4. Cabecera escrita a mano y no por `serialize`, que solo la emite junto
    //    con la primera fila: sin archivos (modo solo carpetas, filtro sin
    //    coincidencias) el CSV quedaría vacío en vez de con sus columnas.
    wtr.write_record(HEADERS)
        .with_context(|| format!("No se pudo escribir el CSV en {}", output.display()))?;

    // 5. Una fila por archivo. `name`, `extension`, `directory` y `full_path`
    //    salen de nombres en disco, fuera del control del usuario (descargas,
    //    carpetas compartidas): se sanean antes de escribir para evitar CSV
    //    injection (`x.=1+1` tiene extensión `=1+1`). Fechas y tamaños los
    //    genera el programa y no son texto libre.
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
        for name in ["=1+1.txt", "-informe.txt", "@evil.txt"] {
            std::fs::write(input.join(name), "x").unwrap();
        }
        let tree = scanner::build_tree(&input, &scanner::ScanOptions::default()).unwrap();

        let output = dir.path().join("report.csv");
        write_csv(&tree, &output).unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.starts_with('\u{feff}'), "falta el BOM UTF-8");
        assert_eq!(content.lines().count(), 4, "cabecera + 3 filas");
        // `name` es la primera columna: cada fila empieza por el nombre saneado.
        assert!(content.contains("\n'=1+1;"));
        assert!(content.contains("\n'-informe;"));
        assert!(content.contains("\n'@evil;"));
    }

    #[test]
    fn write_csv_neutralizes_formula_injection_in_extension() {
        // La extensión también sale del nombre en disco: `x.=1+1` tiene
        // extensión `=1+1`, que Excel evaluaría como fórmula.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        std::fs::create_dir(&input).unwrap();
        std::fs::write(input.join("x.=1+1"), "x").unwrap();
        let tree = scanner::build_tree(&input, &scanner::ScanOptions::default()).unwrap();

        let output = dir.path().join("report.csv");
        write_csv(&tree, &output).unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.contains("\nx;'=1+1;"), "{content}");
    }

    #[test]
    fn write_csv_keeps_the_header_without_files() {
        // Modo solo carpetas o filtro sin coincidencias: sin filas, pero el
        // CSV conserva sus columnas en vez de quedar vacío.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        std::fs::create_dir_all(input.join("sub")).unwrap();
        let tree = scanner::build_tree(&input, &scanner::ScanOptions::default()).unwrap();

        let output = dir.path().join("report.csv");
        write_csv(&tree, &output).unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        assert_eq!(content, format!("\u{feff}{}\n", HEADERS.join(";")));
    }

    #[test]
    fn headers_match_the_serialized_record() {
        // `HEADERS` se escribe a mano: debe seguir coincidiendo con los
        // campos de `FileRecord` si alguno cambia.
        let record = FileRecord {
            name: String::new(),
            extension: String::new(),
            size_mb: String::new(),
            size_bytes: 0,
            created: String::new(),
            modified: String::new(),
            created_iso: String::new(),
            modified_iso: String::new(),
            directory: String::new(),
            full_path: String::new(),
        };
        let mut wtr = writer_builder().from_writer(Vec::new());
        wtr.serialize(&record).unwrap();
        let out = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
        assert_eq!(out.lines().next(), Some(HEADERS.join(";").as_str()));
    }

    #[test]
    fn size_mb_uses_a_decimal_comma() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        std::fs::create_dir(&input).unwrap();
        std::fs::write(input.join("grande.bin"), vec![0u8; 1_500_000]).unwrap();
        let tree = scanner::build_tree(&input, &scanner::ScanOptions::default()).unwrap();

        let output = dir.path().join("report.csv");
        write_csv(&tree, &output).unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.contains("\ngrande;bin;1,431;1500000;"), "{content}");
    }
}
