use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "file-reporter",
    version,
    about = "Genera reportes (CSV, Markdown, JSON) de la estructura de un directorio."
)]
pub struct Cli {
    /// Ruta de la carpeta a escanear (obligatorio).
    pub path: PathBuf,

    /// Extensiones a filtrar (ej: -e txt -e jpg). Si no se pasa, incluye todas.
    #[arg(short, long = "ext", value_delimiter = ',')]
    pub extensions: Vec<String>,

    /// Directorio de salida (por defecto: la misma ruta de entrada).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

impl Cli {
    /// Normaliza las extensiones: minúsculas y sin punto inicial.
    pub fn normalized_extensions(&self) -> Vec<String> {
        self.extensions
            .iter()
            .map(|e| e.trim_start_matches('.').to_lowercase())
            .collect()
    }
}