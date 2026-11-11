use crate::runner::ReportFormat;
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
    /// Incompatible con `--ext`, que no tendría efecto.
    #[arg(short = 'd', long = "dirs-only", conflicts_with = "extensions")]
    pub dirs_only: bool,

    /// Carpetas a omitir por nombre, en cualquier nivel (ej: -x .git -x "mis fotos").
    /// Admite varias separadas por coma.
    #[arg(short = 'x', long = "exclude", value_name = "CARPETA")]
    pub exclude: Vec<String>,

    /// Omitir también las carpetas comunes de proyectos: .git, node_modules,
    /// target, __pycache__, .venv, dist y build.
    #[arg(long = "exclude-common")]
    pub exclude_common: bool,

    /// Reportes a generar (ej: -f csv,md). Si no se pasa, se generan los tres.
    #[arg(short = 'f', long = "format", value_enum, value_delimiter = ',')]
    pub formats: Vec<ReportFormat>,
}

impl Cli {
    /// Formatos pedidos, o todos si no se indicó ninguno.
    pub fn formats_or_all(&self) -> Vec<ReportFormat> {
        if self.formats.is_empty() {
            ReportFormat::ALL.to_vec()
        } else {
            self.formats.clone()
        }
    }

    /// Carpetas a omitir: las de `--exclude-common` (si se pidió) y las de
    /// `--exclude`, con las mismas reglas que la GUI (ver
    /// `runner::parse_dir_names`).
    pub fn excluded_dirs(&self) -> Vec<String> {
        let common = if self.exclude_common {
            crate::runner::COMMON_EXCLUDED_DIRS.as_slice()
        } else {
            &[]
        };
        crate::runner::parse_dir_names(
            common
                .iter()
                .copied()
                .chain(self.exclude.iter().map(String::as_str)),
        )
    }

    /// Normaliza las extensiones con las mismas reglas que la GUI (ver
    /// `runner::parse_extensions`).
    pub fn normalized_extensions(&self) -> Vec<String> {
        crate::runner::parse_extensions(self.extensions.iter().map(String::as_str))
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
            exclude: vec![],
            exclude_common: false,
            formats: vec![],
        }
    }

    #[test]
    fn formats_default_to_all_and_accept_aliases() {
        let cli = Cli::try_parse_from(["file-reporter", "."]).unwrap();
        assert_eq!(cli.formats_or_all(), ReportFormat::ALL.to_vec());

        let cli = Cli::try_parse_from(["file-reporter", ".", "-f", "csv,markdown"]).unwrap();
        assert_eq!(
            cli.formats_or_all(),
            vec![ReportFormat::Csv, ReportFormat::Md]
        );

        assert!(Cli::try_parse_from(["file-reporter", ".", "-f", "pdf"]).is_err());
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

    #[test]
    fn ext_with_spaces_after_commas_is_parsed() {
        let cli = Cli::try_parse_from(["file-reporter", ".", "-e", "pdf, docx"]).unwrap();
        assert_eq!(cli.normalized_extensions(), vec!["pdf", "docx"]);
    }

    #[test]
    fn excluded_dirs_combine_common_and_repeated_flags() {
        let cli = Cli::try_parse_from(["file-reporter", "."]).unwrap();
        assert!(cli.excluded_dirs().is_empty());

        let cli = Cli::try_parse_from([
            "file-reporter",
            ".",
            "-x",
            "mis fotos, tmp",
            "--exclude",
            ".git",
        ])
        .unwrap();
        assert_eq!(cli.excluded_dirs(), vec!["mis fotos", "tmp", ".git"]);

        // `.git` ya está en la lista común: no se repite.
        let cli = Cli::try_parse_from(["file-reporter", ".", "--exclude-common", "-x", ".git,out"])
            .unwrap();
        let dirs = cli.excluded_dirs();
        assert_eq!(dirs.len(), crate::runner::COMMON_EXCLUDED_DIRS.len() + 1);
        assert_eq!(dirs.last().map(String::as_str), Some("out"));
    }

    #[test]
    fn dirs_only_conflicts_with_ext() {
        assert!(Cli::try_parse_from(["file-reporter", ".", "-d", "-e", "txt"]).is_err());
        assert!(Cli::try_parse_from(["file-reporter", ".", "-d"]).is_ok());
    }
}
