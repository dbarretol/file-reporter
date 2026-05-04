use crate::models::FileRecord;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// BOM UTF-8: marca al inicio del archivo que indica a Excel
/// (y otros programas) que el contenido está codificado en UTF-8.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

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
    for r in records {
        wtr.serialize(r)?;
    }

    wtr.flush()?;
    Ok(())
}