//! Tests de la CLI como programa: se ejecuta el binario compilado y se
//! comprueban el código de salida, la salida y los reportes generados.
//!
//! Cargo expone la ruta del binario en `CARGO_BIN_EXE_<nombre>` para los
//! tests de integración, así que no hace falta ninguna dependencia extra.
//! Todas las llamadas llevan argumentos: sin ellos el programa abre la GUI.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn file_reporter(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_file-reporter"))
        .args(args)
        .output()
        .expect("no se pudo ejecutar file-reporter")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Carpeta de entrada con dos archivos en la raíz, uno en `docs` y otro
/// dentro de una carpeta `build`.
fn sample_input(root: &Path) -> std::path::PathBuf {
    let input = root.join("in");
    fs::create_dir_all(input.join("docs")).unwrap();
    fs::create_dir_all(input.join("build")).unwrap();
    fs::write(input.join("a.txt"), "hola").unwrap();
    fs::write(input.join("b.pdf"), "pdf").unwrap();
    fs::write(input.join("docs").join("c.txt"), "x").unwrap();
    fs::write(input.join("build").join("app.exe"), "x").unwrap();
    input
}

#[test]
fn version_prints_the_package_version() {
    let o = file_reporter(&["--version"]);
    assert!(o.status.success());
    assert!(stdout(&o).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn generates_the_three_reports_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let input = sample_input(dir.path());
    let out = dir.path().join("out");

    let o = file_reporter(&[s(&input), "-o", s(&out)]);

    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    assert!(
        text.contains("✅ Reportes generados correctamente"),
        "{text}"
    );
    assert!(text.contains("📊 Archivos procesados: 4"), "{text}");
    assert!(text.contains("⏱️  Tiempo:"), "{text}");
    for name in ["report.csv", "report.md", "report.json"] {
        assert!(out.join(name).is_file(), "falta {name}");
    }
}

#[test]
fn format_option_limits_the_reports() {
    let dir = tempfile::tempdir().unwrap();
    let input = sample_input(dir.path());
    let out = dir.path().join("out");

    let o = file_reporter(&[s(&input), "-o", s(&out), "-f", "json,csv"]);

    assert!(o.status.success(), "{}", stderr(&o));
    assert!(out.join("report.csv").is_file());
    assert!(out.join("report.json").is_file());
    assert!(!out.join("report.md").exists());
}

#[test]
fn extension_filter_counts_only_matching_files() {
    let dir = tempfile::tempdir().unwrap();
    let input = sample_input(dir.path());
    let out = dir.path().join("out");

    let o = file_reporter(&[s(&input), "-o", s(&out), "-e", ".TXT", "-f", "csv"]);

    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    assert!(text.contains("🎯 Filtrando extensiones: txt"), "{text}");
    assert!(text.contains("📊 Archivos procesados: 2"), "{text}");
    let csv = fs::read_to_string(out.join("report.csv")).unwrap();
    assert_eq!(csv.lines().count(), 3, "cabecera + 2 filas:\n{csv}");
}

#[test]
fn dirs_only_reports_folders_and_keeps_the_csv_header() {
    let dir = tempfile::tempdir().unwrap();
    let input = sample_input(dir.path());
    let out = dir.path().join("out");

    let o = file_reporter(&[s(&input), "-o", s(&out), "-d"]);

    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    assert!(
        text.contains("📂 Modo: solo estructura de carpetas"),
        "{text}"
    );
    assert!(text.contains("📊 Carpetas procesadas: 2"), "{text}");
    let csv = fs::read_to_string(out.join("report.csv")).unwrap();
    assert_eq!(csv.lines().count(), 1, "solo la cabecera:\n{csv}");
}

#[test]
fn dirs_only_and_extensions_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let o = file_reporter(&[s(dir.path()), "-d", "-e", "pdf"]);
    // Error de uso de clap: código 2 y ningún reporte.
    assert_eq!(o.status.code(), Some(2));
    assert!(!dir.path().join("report.csv").exists());
}

#[test]
fn exclude_omits_folders_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let input = sample_input(dir.path());
    let out = dir.path().join("out");

    let o = file_reporter(&[s(&input), "-o", s(&out), "-x", "build"]);

    assert!(o.status.success(), "{}", stderr(&o));
    let text = stdout(&o);
    assert!(text.contains("🚫 Omitiendo carpetas: build"), "{text}");
    assert!(text.contains("📊 Archivos procesados: 3"), "{text}");
    assert!(
        text.contains("🚫 1 carpeta omitida por nombre (build)"),
        "{text}"
    );
    let md = fs::read_to_string(out.join("report.md")).unwrap();
    assert!(!md.contains("app.exe"));
}

#[test]
fn verbose_logs_what_was_omitted_to_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let input = sample_input(dir.path());
    let out = dir.path().join("out");

    let o = file_reporter(&[s(&input), "-o", s(&out), "-x", "build", "-v"]);

    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stderr(&o).contains("Carpeta omitida por nombre"),
        "{}",
        stderr(&o)
    );
    // El registro va a stderr: la salida normal no cambia.
    assert!(!stdout(&o).contains("Carpeta omitida por nombre"));
}

#[test]
fn missing_folder_fails_with_a_one_line_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no_existe");

    let o = file_reporter(&[s(&missing)]);

    assert_eq!(o.status.code(), Some(1));
    let err = stderr(&o);
    assert!(err.starts_with("❌ La ruta no existe:"), "{err}");
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(!err.contains("Caused by"), "{err}");
}

#[test]
fn error_causes_are_shown_on_the_same_line() {
    // La "carpeta" de salida es un archivo: falla al crearla, y el mensaje
    // incluye la causa del sistema en la misma línea.
    let dir = tempfile::tempdir().unwrap();
    let input = sample_input(dir.path());
    let blocked = dir.path().join("bloqueado");
    fs::write(&blocked, "soy un archivo").unwrap();

    let o = file_reporter(&[s(&input), "-o", s(&blocked)]);

    assert_eq!(o.status.code(), Some(1));
    let err = stderr(&o);
    assert!(
        err.starts_with("❌ No se pudo crear el directorio de salida"),
        "{err}"
    );
    assert_eq!(err.lines().count(), 1, "{err}");
    // `{:#}` de anyhow separa cada causa con ": ".
    let message = err.trim_end();
    assert!(message.matches(": ").count() >= 1, "{err}");
}
