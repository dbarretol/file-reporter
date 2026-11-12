//! Textos del resumen de una corrida que comparten la CLI y la GUI:
//! tamaños, porcentajes, tipos de archivo, progreso y avisos de lo omitido.
//! `runner` ejecuta; este módulo solo da formato.

use crate::models::ExtStat;

/// Tamaño legible para el usuario, en unidades binarias (1 KB = 1024 B):
/// `512 B`, `1.5 KB`, `2.3 GB`.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Porcentaje legible con un decimal (`25.0 %`); `<0.1 %` para partes no
/// nulas muy pequeñas, que si no se mostrarían como `0.0 %`.
pub fn format_percent(part: u64, total: u64) -> String {
    if total == 0 {
        return "0 %".to_string();
    }
    let p = part as f64 * 100.0 / total as f64;
    if part > 0 && p < 0.1 {
        "<0.1 %".to_string()
    } else {
        format!("{p:.1} %")
    }
}

/// Fila del resumen de tipos de archivo que muestran la CLI y la GUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRow {
    pub label: String,
    pub count: usize,
    pub bytes: u64,
}

/// Los `n` tipos con más tamaño y, si quedan más, una fila "Otros (k tipos)"
/// que los suma. Si solo sobraría uno, se muestra tal cual en vez de
/// agruparlo. `stats` debe venir ordenado (como `RunSummary::ext_stats`).
pub fn top_types(stats: &[ExtStat], n: usize) -> Vec<TypeRow> {
    let row = |s: &ExtStat| TypeRow {
        label: s.label(),
        count: s.count,
        bytes: s.bytes,
    };
    if stats.len() <= n + 1 {
        return stats.iter().map(row).collect();
    }
    let (top, rest) = stats.split_at(n);
    let mut rows: Vec<TypeRow> = top.iter().map(row).collect();
    rows.push(TypeRow {
        label: format!("Otros ({} tipos)", rest.len()),
        count: rest.iter().map(|s| s.count).sum(),
        bytes: rest.iter().map(|s| s.bytes).sum(),
    });
    rows
}

/// Qué cuenta `Progress::done`, para los textos de progreso de la CLI y la GUI.
pub fn found_noun(dirs_only: bool) -> &'static str {
    if dirs_only {
        "carpetas encontradas"
    } else {
        "archivos encontrados"
    }
}

/// Estado del escaneo en curso para la CLI y la GUI: lo encontrado y, salvo
/// en modo solo carpetas (donde ya se cuentan carpetas), las carpetas
/// revisadas. Con un filtro de extensiones lo encontrado puede quedarse en 0
/// mucho tiempo; las carpetas revisadas muestran que el escaneo avanza.
pub fn scan_status(found: usize, dirs_scanned: usize, dirs_only: bool) -> String {
    let noun = found_noun(dirs_only);
    if dirs_only {
        format!("{found} {noun}")
    } else {
        format!("{found} {noun} · {dirs_scanned} carpetas revisadas")
    }
}

/// Aviso de las carpetas omitidas por nombre, con los nombres que
/// coincidieron: "3 carpetas omitidas por nombre (.git, build ×2)". `None`
/// si no se omitió ninguna.
pub fn excluded_dirs_text(excluded: &[(String, usize)]) -> Option<String> {
    let total: usize = excluded.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return None;
    }
    let names: Vec<String> = excluded
        .iter()
        .map(|(name, n)| {
            if *n > 1 {
                format!("{name} ×{n}")
            } else {
                name.clone()
            }
        })
        .collect();
    let noun = if total == 1 {
        "carpeta omitida"
    } else {
        "carpetas omitidas"
    };
    Some(format!("{total} {noun} por nombre ({})", names.join(", ")))
}

/// Aviso de los elementos que no se pudieron leer.
pub fn skipped_text(count: usize) -> String {
    if count == 1 {
        "1 elemento no accesible fue omitido".to_string()
    } else {
        format!("{count} elementos no accesibles fueron omitidos")
    }
}

/// Duración para mostrar al usuario, igual en la CLI y en la GUI: `<1 s`,
/// `42s`, `3m 05s` o `1h 02m 03s`.
pub fn fmt_seconds(s: f32) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_picks_a_readable_unit() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(2_469_606_195), "2.3 GB");
    }

    #[test]
    fn format_percent_handles_zero_and_tiny_values() {
        assert_eq!(format_percent(0, 0), "0 %");
        assert_eq!(format_percent(1, 4), "25.0 %");
        assert_eq!(format_percent(1, 10_000), "<0.1 %");
        assert_eq!(format_percent(0, 10), "0.0 %");
    }

    fn stat(ext: &str, count: usize, bytes: u64) -> ExtStat {
        ExtStat {
            extension: ext.to_string(),
            count,
            bytes,
        }
    }

    #[test]
    fn top_types_groups_the_rest_under_otros() {
        let stats = [
            stat("pdf", 1, 100),
            stat("jpg", 2, 50),
            stat("txt", 3, 20),
            stat("", 4, 10),
        ];
        let rows = top_types(&stats, 2);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].label, ".pdf");
        assert_eq!(
            rows[2],
            TypeRow {
                label: "Otros (2 tipos)".to_string(),
                count: 7,
                bytes: 30,
            }
        );
    }

    #[test]
    fn top_types_does_not_group_a_single_leftover() {
        let stats = [stat("pdf", 1, 100), stat("jpg", 2, 50), stat("txt", 3, 20)];
        let labels: Vec<String> = top_types(&stats, 2).into_iter().map(|r| r.label).collect();
        assert_eq!(labels, vec![".pdf", ".jpg", ".txt"]);
        assert!(top_types(&[], 5).is_empty());
    }

    #[test]
    fn scan_status_adds_scanned_folders_unless_counting_folders() {
        assert_eq!(
            scan_status(120, 3400, false),
            "120 archivos encontrados · 3400 carpetas revisadas"
        );
        assert_eq!(scan_status(7, 8, true), "7 carpetas encontradas");
    }

    #[test]
    fn excluded_dirs_text_lists_names_and_repeats() {
        assert_eq!(excluded_dirs_text(&[]), None);
        assert_eq!(
            excluded_dirs_text(&[("build".to_string(), 1)]).as_deref(),
            Some("1 carpeta omitida por nombre (build)")
        );
        assert_eq!(
            excluded_dirs_text(&[(".git".to_string(), 1), ("build".to_string(), 2)]).as_deref(),
            Some("3 carpetas omitidas por nombre (.git, build ×2)")
        );
    }

    #[test]
    fn skipped_text_agrees_in_number() {
        assert_eq!(skipped_text(1), "1 elemento no accesible fue omitido");
        assert_eq!(skipped_text(4), "4 elementos no accesibles fueron omitidos");
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
}
