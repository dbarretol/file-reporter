use crate::models::FileRecord;
use anyhow::{Context, Result};
use std::path::Path;

pub fn write_csv(records: &[FileRecord], output: &Path) -> Result<()> {
    let mut wtr = csv::Writer::from_path(output)
        .with_context(|| format!("No se pudo crear CSV en {}", output.display()))?;
    for r in records {
        wtr.serialize(r)?;
    }
    wtr.flush()?;
    Ok(())
}