//! Funciones auxiliares sin dependencia de egui: texto, rutas y apertura de
//! archivos con el programa del sistema.

use std::path::{Path, PathBuf};

/// Limpia una ruta pegada por el usuario: quita espacios y, si vienen,
/// las comillas que agrega "Copiar como ruta de acceso" de Windows.
pub(super) fn clean_path_input(s: &str) -> &str {
    s.trim().trim_matches('"')
}

/// El campo "carpeta a escanear" espera un directorio. Si el usuario suelta
/// un archivo suelto (no una carpeta) por drag & drop, usar su ruta tal
/// cual dejaría un `input_path` que `runner::run` rechazaría con "La ruta
/// no es un directorio" — en su lugar, usamos la carpeta que lo contiene.
pub(super) fn dropped_path_to_input_dir(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        path
    } else {
        path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
    }
}

/// Trunca un string a `max_chars` caracteres (no bytes), prefijando "…"
/// si fue acortado. Seguro con UTF-8.
pub(super) fn truncate_path_display(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    // Tomar los últimos (max_chars - 1) caracteres y prefijar con "…"
    let skip = char_count - (max_chars - 1);
    let tail: String = s.chars().skip(skip).collect();
    format!("…{}", tail)
}

pub(super) fn fmt_seconds(s: f32) -> String {
    // "0s" en un escaneo rápido parecía un error.
    if s < 1.0 {
        return "<1 s".to_string();
    }
    let s = s as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{h}h {m:02}m {sec:02}s")
    } else if m > 0 {
        format!("{m}m {sec:02}s")
    } else {
        format!("{sec}s")
    }
}

#[cfg(target_os = "windows")]
pub(super) fn open_in_explorer(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn open_in_explorer(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    std::process::Command::new(cmd)
        .arg(path)
        .spawn()
        .map(|_| ())
}

/// Abre la carpeta contenedora de `path` con el archivo ya resaltado
/// (en vez de una carpeta "en blanco" sin selección).
#[cfg(target_os = "windows")]
pub(super) fn open_selecting(path: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    // El Explorador separa sus argumentos por comas: sin comillas, una ruta
    // como `D:\docs,v2\report.csv` abre otra carpeta (la de Documentos). Va
    // con `raw_arg` para que Rust no vuelva a escapar las comillas; una ruta
    // de Windows no puede contener `"`.
    std::process::Command::new("explorer")
        .raw_arg(select_arg(path))
        .spawn()
        .map(|_| ())
}

/// Argumento `/select,"<ruta>"` del Explorador de Windows.
#[cfg(any(target_os = "windows", test))]
fn select_arg(path: &Path) -> String {
    format!("/select,\"{}\"", path.display())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn open_selecting(path: &Path) -> std::io::Result<()> {
    // No hay un equivalente universal a "/select," fuera de Windows;
    // abrimos la carpeta contenedora sin resaltar el archivo.
    match path.parent() {
        Some(parent) => open_in_explorer(parent),
        None => open_in_explorer(path),
    }
}

/// Carpetas recientes que se recuerdan.
pub(super) const MAX_RECENT: usize = 5;

/// Pone `path` al principio de la lista de recientes, sin repetirla (una
/// misma carpeta escrita con otra barra final o, en Windows y macOS, con
/// otras mayúsculas cuenta como la misma) y sin pasar de `MAX_RECENT`.
pub(super) fn push_recent(list: &mut Vec<String>, path: &str) {
    let path = path.trim_end_matches(['/', '\\']);
    if path.is_empty() {
        return;
    }
    let key = recent_key(path);
    list.retain(|p| recent_key(p) != key);
    list.insert(0, path.to_string());
    list.truncate(MAX_RECENT);
}

fn recent_key(path: &str) -> String {
    let path = path.trim_end_matches(['/', '\\']);
    if cfg!(any(windows, target_os = "macos")) {
        path.to_lowercase()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_recent_moves_repeats_to_the_front_and_keeps_five() {
        let mut list = Vec::new();
        for p in ["a", "b", "c", "d", "e", "f"] {
            push_recent(&mut list, p);
        }
        assert_eq!(list, vec!["f", "e", "d", "c", "b"]);

        push_recent(&mut list, "c/");
        assert_eq!(list, vec!["c", "f", "e", "d", "b"]);

        push_recent(&mut list, "");
        assert_eq!(list.len(), MAX_RECENT);
    }

    #[cfg(windows)]
    #[test]
    fn push_recent_ignores_case_on_windows() {
        let mut list = vec![r"C:\Datos".to_string()];
        push_recent(&mut list, r"c:\datos\");
        assert_eq!(list, vec![r"c:\datos"]);
    }

    #[test]
    fn select_arg_quotes_paths_with_commas_and_spaces() {
        assert_eq!(
            select_arg(Path::new(r"D:\docs,v2\mi reporte\report.csv")),
            r#"/select,"D:\docs,v2\mi reporte\report.csv""#
        );
    }

    #[test]
    fn truncate_path_display_noop_when_short_enough() {
        assert_eq!(truncate_path_display("short", 10), "short");
        assert_eq!(truncate_path_display("exact", 5), "exact");
    }

    #[test]
    fn truncate_path_display_truncates_from_the_front() {
        let s = "C:\\Users\\someone\\Documents\\file.txt";
        let out = truncate_path_display(s, 10);
        assert_eq!(out.chars().count(), 10);
        assert!(out.starts_with('…'));
        let tail: String = out.chars().skip(1).collect();
        assert!(s.ends_with(&tail));
    }

    #[test]
    fn truncate_path_display_respects_utf8_char_boundaries() {
        // Cadena con multibyte (acentos, emoji) cerca del punto de corte:
        // debe truncar por caracteres, no por bytes, sin panickear.
        let s = "café_😀_carpeta_con_ñ_y_más_texto.txt";
        let out = truncate_path_display(s, 12);
        assert_eq!(out.chars().count(), 12);
        assert!(out.starts_with('…'));
    }

    #[test]
    fn fmt_seconds_boundaries() {
        assert_eq!(fmt_seconds(0.0), "<1 s");
        assert_eq!(fmt_seconds(0.4), "<1 s");
        assert_eq!(fmt_seconds(1.0), "1s");
        assert_eq!(fmt_seconds(59.0), "59s");
        assert_eq!(fmt_seconds(60.0), "1m 00s");
        assert_eq!(fmt_seconds(3599.0), "59m 59s");
        assert_eq!(fmt_seconds(3600.0), "1h 00m 00s");
        assert_eq!(fmt_seconds(3661.0), "1h 01m 01s");
    }

    #[test]
    fn dropped_path_to_input_dir_keeps_directories_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();
        assert_eq!(dropped_path_to_input_dir(dir_path.clone()), dir_path);
    }

    #[test]
    fn dropped_path_to_input_dir_uses_parent_for_files() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("dropped.txt");
        std::fs::write(&file_path, "x").unwrap();
        assert_eq!(
            dropped_path_to_input_dir(file_path),
            dir.path().to_path_buf()
        );
    }
}
