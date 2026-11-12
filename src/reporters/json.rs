use crate::models::Node;
use anyhow::{Context, Result};
use std::path::Path;

/// Escribe el árbol completo en JSON, a través de un temporal que solo
/// reemplaza al reporte anterior si todo va bien (ver
/// `reporters::write_atomic`).
pub fn write_json(root: &Node, output: &Path) -> Result<()> {
    super::write_atomic(output, "JSON", |writer| {
        serde_json::to_writer_pretty(writer, root)
            .with_context(|| format!("No se pudo escribir JSON en {}", output.display()))
    })
}
