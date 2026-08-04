use crate::models::Node;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn write_json(root: &Node, output: &Path) -> Result<()> {
    let file = File::create(output)
        .with_context(|| format!("No se pudo crear JSON en {}", output.display()))?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, root)?;
    Ok(())
}
