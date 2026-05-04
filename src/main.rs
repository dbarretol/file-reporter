mod cli;
mod models;
mod reporters;
mod scanner;

use anyhow::{bail, Context, Result};
use clap::Parser;
use cli::Cli;
use std::fs;
use std::time::Instant;

fn main() -> Result<()> {
    let args = Cli::parse();

    // Validar entrada
    if !args.path.exists() {
        bail!("❌ La ruta no existe: {}", args.path.display());
    }
    if !args.path.is_dir() {
        bail!("❌ La ruta no es un directorio: {}", args.path.display());
    }

    // Directorio de salida
    let output_dir = args.output.clone().unwrap_or_else(|| args.path.clone());
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("No se pudo crear el directorio de salida {}", output_dir.display()))?;

    let filters = args.normalized_extensions();
    if !filters.is_empty() {
        println!("🎯 Filtrando extensiones: {:?}", filters);
    }
    println!("🔍 Escaneando: {}", args.path.display());

    let start = Instant::now();

    // 1. Construir árbol
    let tree = scanner::build_tree(&args.path, &filters)
        .context("Error al construir el árbol de directorios")?;

    // 2. Aplanar para CSV
    let mut records = Vec::new();
    scanner::flatten_to_records(&tree, &filters, &mut records);

    // 3. Generar reportes
    let csv_path = output_dir.join("report.csv");
    let md_path = output_dir.join("report.md");
    let json_path = output_dir.join("report.json");

    reporters::csv::write_csv(&records, &csv_path)?;
    reporters::markdown::write_markdown(&tree, &md_path)?;
    reporters::json::write_json(&tree, &json_path)?;

    let elapsed = start.elapsed();

    println!("\n✅ Reportes generados correctamente:");
    println!("   📄 CSV:      {}", csv_path.display());
    println!("   📄 Markdown: {}", md_path.display());
    println!("   📄 JSON:     {}", json_path.display());
    println!("\n📊 Archivos procesados: {}", records.len());
    println!("⏱️  Tiempo: {:.2?}", elapsed);

    Ok(())
}