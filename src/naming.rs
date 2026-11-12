//! Nombre de los reportes: el nombre base por defecto, la validación de un
//! nombre elegido por el usuario y el nombre automático de "Guardar con otro
//! nombre", que también sirve para reconocer esos reportes al volver a
//! escanear la carpeta.

use crate::runner::ReportFormat;
use anyhow::{bail, Result};
use std::path::Path;

/// Nombre base de los reportes si no se elige otro.
pub const DEFAULT_REPORT_NAME: &str = "report";

/// Caracteres que no pueden ir en un nombre de archivo (en Windows; en
/// Linux y macOS solo `/`, pero así un mismo nombre vale en todos).
const INVALID_NAME_CHARS: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Comprueba que `name` sirva como nombre base de los reportes: no vacío,
/// sin separadores de carpeta ni caracteres no válidos en Windows, y sin
/// punto o espacio al final (Windows los quita en silencio).
pub fn validate_report_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("El nombre de los reportes no puede estar vacío");
    }
    if let Some(c) = name
        .chars()
        .find(|c| INVALID_NAME_CHARS.contains(c) || c.is_control())
    {
        bail!("El nombre de los reportes no puede contener «{c}»: {name}");
    }
    if name.ends_with(['.', ' ']) {
        bail!("El nombre de los reportes no puede terminar en punto ni en espacio: {name}");
    }
    Ok(())
}

/// Nombre alternativo para no pisar reportes existentes:
/// `report-<carpeta>-<AAAA-MM-DD>` y, si alguno de los `formats` ya existe
/// en `output` con ese nombre, `-2`, `-3`… La carpeta es el último
/// componente de `input` (la letra de unidad para `C:\\`), con los
/// caracteres no válidos cambiados por `_`.
pub fn auto_report_name(
    input: &Path,
    output: &Path,
    formats: &[ReportFormat],
    date: chrono::NaiveDate,
) -> String {
    let folder: String = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.display().to_string())
        .chars()
        .map(|c| {
            if INVALID_NAME_CHARS.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let folder = folder.trim_matches(['_', '.', ' ']);
    let folder = if folder.is_empty() { "carpeta" } else { folder };
    let base = format!("{DEFAULT_REPORT_NAME}-{folder}-{}", date.format("%Y-%m-%d"));
    let taken = |name: &str| {
        formats
            .iter()
            .any(|f| output.join(f.file_name(name)).exists())
    };
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|name| !taken(name))
        .expect("siempre hay un número libre")
}

/// `true` si `file_name` es un reporte con nombre automático (ver
/// `auto_report_name`) o su temporal: `report-<carpeta>-AAAA-MM-DD[-N]`
/// con extensión `.csv`, `.md` o `.json` (y quizá `.tmp`). Sirve para no
/// incluir en un reporte los de corridas anteriores guardadas con otro
/// nombre.
pub fn is_auto_report_name(file_name: &str) -> bool {
    let name = file_name.strip_suffix(".tmp").unwrap_or(file_name);
    let Some(stem) = ReportFormat::ALL
        .iter()
        .find_map(|f| name.strip_suffix(&format!(".{}", f.extension())))
    else {
        return false;
    };
    let Some(rest) = stem.strip_prefix(&format!("{DEFAULT_REPORT_NAME}-")) else {
        return false;
    };
    // `<carpeta>-AAAA-MM-DD`, con la carpeta no vacía.
    let ends_with_date = |s: &str| {
        s.len() > 11 && {
            let (folder, date) = s.split_at(s.len() - 11);
            !folder.is_empty()
                && date.starts_with('-')
                && chrono::NaiveDate::parse_from_str(&date[1..], "%Y-%m-%d").is_ok()
        }
    };
    if ends_with_date(rest) {
        return true;
    }
    // Con sufijo `-N`.
    match rest.rsplit_once('-') {
        Some((head, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            ends_with_date(head)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn validate_report_name_accepts_normal_names_only() {
        for ok in ["report", "facturas 2026", "inventario.v2", "año-ñandú"] {
            assert!(validate_report_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "  ", "a/b", r"a\b", "a:b", "a*", "a?", "a\"b", "<a>", "a|b", "a.", "a ",
        ] {
            assert!(validate_report_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn auto_report_name_uses_folder_and_date_and_skips_taken_names() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path();
        let date = chrono::NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let input = Path::new("D:/datos/Mis docs");
        let formats = [ReportFormat::Csv, ReportFormat::Json];

        assert_eq!(
            auto_report_name(input, out, &formats, date),
            "report-Mis docs-2026-11-12"
        );
        // Basta con que exista uno de los formatos pedidos para pasar al
        // siguiente número.
        fs::write(out.join("report-Mis docs-2026-11-12.json"), "").unwrap();
        assert_eq!(
            auto_report_name(input, out, &formats, date),
            "report-Mis docs-2026-11-12-2"
        );
        fs::write(out.join("report-Mis docs-2026-11-12-2.csv"), "").unwrap();
        assert_eq!(
            auto_report_name(input, out, &formats, date),
            "report-Mis docs-2026-11-12-3"
        );
        // Un formato no pedido no ocupa el nombre.
        assert_eq!(
            auto_report_name(input, out, &[ReportFormat::Md], date),
            "report-Mis docs-2026-11-12"
        );
    }

    #[test]
    fn auto_report_name_handles_drive_roots() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let name = auto_report_name(Path::new(r"C:\"), dir.path(), &ReportFormat::ALL, date);
        assert!(validate_report_name(&name).is_ok(), "{name}");
        assert!(name.ends_with("-2026-01-02"), "{name}");
    }

    #[test]
    fn is_auto_report_name_matches_only_generated_names() {
        for yes in [
            "report-docs-2026-11-12.csv",
            "report-docs-2026-11-12-2.md",
            "report-mis-docs-2026-11-12.json",
            "report-docs-2026-11-12.csv.tmp",
        ] {
            assert!(is_auto_report_name(yes), "{yes}");
        }
        for no in [
            "report.csv",
            "report-final.csv",
            "report-2026-11-12.csv",
            "report-docs-2026-11-12.txt",
            "report-docs-2026-13-40.csv",
            "reporte-docs-2026-11-12.csv",
            "report-docs-2026-11-12-.csv",
        ] {
            assert!(!is_auto_report_name(no), "{no}");
        }
    }
}
