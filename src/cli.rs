use crate::runner::{self, Progress, ReportFormat, RunConfig, RunSummary};
use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

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

    /// Mostrar qué se omitió y por qué: carpetas y archivos sin acceso,
    /// enlaces no seguidos y carpetas excluidas por nombre. Con -vv, también
    /// el detalle interno del proceso.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbose: u8,
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

/// Activa el registro detallado en stderr según el número de `-v`: nada sin
/// `-v` (la salida normal ya resume lo importante), avisos e información con
/// `-v` y además depuración con `-vv`.
pub fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => return,
        1 => tracing::Level::INFO,
        _ => tracing::Level::DEBUG,
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        // Sin colores si stderr va a un archivo, para no ensuciarlo.
        .with_ansi(std::io::stderr().is_terminal())
        .with_target(false)
        .without_time()
        .init();
}

/// Cada cuánto se refresca el spinner (giro y contador).
const SPINNER_TICK: Duration = Duration::from_millis(100);

/// Ejecuta `runner::run` mostrando en stderr un spinner con el contador de
/// elementos encontrados y el tiempo transcurrido, y después la fase de
/// escritura de reportes. Usa el mismo `runner::Progress` que la GUI.
///
/// Sin spinner si `show` es `false` (con `-v`, para que no se mezcle con las
/// líneas del registro, que también van a stderr) o si stderr no es una
/// terminal (salida redirigida, CI…): entonces la ejecución es la normal.
pub fn run_with_progress(cfg: &RunConfig, show: bool) -> Result<RunSummary> {
    let bar = ProgressBar::new_spinner();
    if !show || bar.is_hidden() {
        return runner::run(cfg, None);
    }
    bar.set_style(
        ProgressStyle::with_template("{spinner} {msg} · {elapsed}")
            .expect("plantilla de spinner válida"),
    );
    bar.enable_steady_tick(SPINNER_TICK);

    let progress = Progress::new();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let result = std::thread::scope(|s| {
        // Actualiza el mensaje hasta que `runner::run` termina (al soltarse
        // `done_tx` el canal se cierra y `recv_timeout` deja de esperar).
        let (bar, watched) = (&bar, progress.clone());
        s.spawn(move || loop {
            bar.set_message(progress_message(&watched, cfg.dirs_only));
            if done_rx.recv_timeout(SPINNER_TICK) != Err(mpsc::RecvTimeoutError::Timeout) {
                break;
            }
        });
        let result = runner::run(cfg, Some(progress.clone()));
        drop(done_tx);
        result
    });
    bar.finish_and_clear();
    result
}

/// Texto del spinner según la fase en curso.
fn progress_message(progress: &Progress, dirs_only: bool) -> String {
    let found = progress.done.load(Ordering::Relaxed);
    if progress.writing.load(Ordering::Relaxed) {
        let noun = runner::found_noun(dirs_only);
        format!("Escribiendo reportes… ({found} {noun})")
    } else {
        let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
        format!(
            "Escaneando… {}",
            runner::scan_status(found, dirs, dirs_only)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_message_shows_phase_and_noun() {
        let p = Progress::default();
        p.done.store(42, Ordering::Relaxed);
        p.dirs_scanned.store(9, Ordering::Relaxed);
        assert_eq!(
            progress_message(&p, false),
            "Escaneando… 42 archivos encontrados · 9 carpetas revisadas"
        );
        assert_eq!(
            progress_message(&p, true),
            "Escaneando… 42 carpetas encontradas"
        );
        p.writing.store(true, Ordering::Relaxed);
        assert_eq!(
            progress_message(&p, false),
            "Escribiendo reportes… (42 archivos encontrados)"
        );
    }

    fn cli_with(extensions: Vec<&str>) -> Cli {
        Cli {
            path: PathBuf::from("."),
            extensions: extensions.into_iter().map(String::from).collect(),
            output: None,
            dirs_only: false,
            exclude: vec![],
            exclude_common: false,
            formats: vec![],
            verbose: 0,
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
    fn verbose_counts_repetitions() {
        let verbose = |args: &[&str]| {
            Cli::try_parse_from(["file-reporter", "."].iter().chain(args))
                .unwrap()
                .verbose
        };
        assert_eq!(verbose(&[]), 0);
        assert_eq!(verbose(&["-v"]), 1);
        assert_eq!(verbose(&["-vv"]), 2);
        assert_eq!(verbose(&["--verbose", "-v"]), 2);
    }

    #[test]
    fn dirs_only_conflicts_with_ext() {
        assert!(Cli::try_parse_from(["file-reporter", ".", "-d", "-e", "txt"]).is_err());
        assert!(Cli::try_parse_from(["file-reporter", ".", "-d"]).is_ok());
    }
}
