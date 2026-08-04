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

    /// Solo escanear la estructura de carpetas (omitir archivos).
    #[arg(short = 'd', long = "dirs-only")]
    pub dirs_only: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_with(extensions: Vec<&str>) -> Cli {
        Cli {
            path: PathBuf::from("."),
            extensions: extensions.into_iter().map(String::from).collect(),
            output: None,
            dirs_only: false,
        }
    }

    #[test]
    fn normalized_extensions_strips_leading_dot() {
        let cli = cli_with(vec![".txt", "jpg", ".PDF"]);
        assert_eq!(cli.normalized_extensions(), vec!["txt", "jpg", "pdf"]);
    }

    #[test]
    fn normalized_extensions_lowercases() {
        let cli = cli_with(vec!["TXT", "Jpg"]);
        assert_eq!(cli.normalized_extensions(), vec!["txt", "jpg"]);
    }

    #[test]
    fn normalized_extensions_empty_stays_empty() {
        let cli = cli_with(vec![]);
        assert!(cli.normalized_extensions().is_empty());
    }
}
