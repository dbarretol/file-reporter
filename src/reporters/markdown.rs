use crate::models::{ExtStat, Node};
use crate::summary::{format_percent, format_size};
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Escribe el árbol de directorios y, si hay archivos, una tabla con los
/// tipos de archivo (`stats`, ya ordenada de mayor a menor tamaño).
pub fn write_markdown(root: &Node, stats: &[ExtStat], output: &Path) -> Result<()> {
    let mut tree = String::new();
    match root {
        Node::Directory { path, children, .. } => {
            tree.push_str(&format!("{}\n", path.display()));
            render_children(children, "", &mut tree);
        }
        Node::File { path, .. } => {
            tree.push_str(&format!("{}\n", path.display()));
        }
    }

    let fence = code_fence(&tree);
    let mut buf = String::with_capacity(tree.len() + 512);
    buf.push_str("# 📋 Estructura de Directorios\n\n");
    buf.push_str(&fence);
    buf.push('\n');
    buf.push_str(&tree);
    buf.push_str(&fence);
    buf.push('\n');
    render_stats(stats, &mut buf);
    // A través de un temporal: un fallo no deja el reporte anterior vacío
    // (ver `reporters::write_atomic`).
    super::write_atomic(output, "Markdown", |w| {
        w.write_all(buf.as_bytes())
            .with_context(|| format!("No se pudo escribir Markdown en {}", output.display()))
    })
}

/// Delimitador del bloque de código del árbol: tres acentos graves o, si el
/// contenido ya tiene una racha así (un nombre de archivo con <code>```</code>,
/// válido en Windows y Linux), uno más que la racha más larga, para que
/// ese nombre no cierre el bloque antes de tiempo (CommonMark).
fn code_fence(content: &str) -> String {
    let longest = content.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

fn render_children(children: &[Node], prefix: &str, buf: &mut String) {
    let total = children.len();
    for (i, child) in children.iter().enumerate() {
        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let new_prefix = if is_last { "    " } else { "│   " };

        buf.push_str(&format!("{}{}{}\n", prefix, connector, child.name()));

        if let Node::Directory { children, .. } = child {
            let next_prefix = format!("{}{}", prefix, new_prefix);
            render_children(children, &next_prefix, buf);
        }
    }
}

fn render_stats(stats: &[ExtStat], buf: &mut String) {
    if stats.is_empty() {
        return;
    }
    let total: u64 = stats.iter().map(|s| s.bytes).sum();
    buf.push_str("\n## 📊 Tipos de archivo\n\n");
    buf.push_str("| Extensión | Archivos | Tamaño | % del tamaño |\n");
    buf.push_str("|---|--:|--:|--:|\n");
    for s in stats {
        // Un `|` en la extensión (posible en Linux) rompería la tabla.
        let label = s.label().replace('|', r"\|");
        buf.push_str(&format!(
            "| {label} | {} | {} | {} |\n",
            s.count,
            format_size(s.bytes),
            format_percent(s.bytes, total)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_fence_is_longer_than_any_backtick_run() {
        assert_eq!(code_fence("a\nb\n"), "```");
        assert_eq!(code_fence("un `nombre`\n"), "```");
        assert_eq!(code_fence("raro```.txt\n"), "````");
        assert_eq!(code_fence("x`````y\n"), "``````");
    }

    #[test]
    fn backticks_in_names_do_not_close_the_tree_block() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        std::fs::create_dir(&input).unwrap();
        std::fs::write(input.join("raro```.txt"), "x").unwrap();
        let tree =
            crate::scanner::build_tree(&input, &crate::scanner::ScanOptions::default()).unwrap();

        let output = dir.path().join("report.md");
        write_markdown(&tree, &[], &output).unwrap();

        let md = std::fs::read_to_string(&output).unwrap();
        // El bloque abre y cierra con cuatro acentos graves, y el nombre
        // queda dentro.
        let lines: Vec<&str> = md.lines().collect();
        assert_eq!(lines[2], "````");
        assert_eq!(lines[4], "└── raro```.txt");
        assert_eq!(lines[5], "````");
    }

    #[test]
    fn stats_table_lists_types_and_escapes_pipes() {
        let stats = vec![
            ExtStat {
                extension: "pdf".into(),
                count: 3,
                bytes: 3072,
            },
            ExtStat {
                extension: "a|b".into(),
                count: 1,
                bytes: 1024,
            },
        ];
        let mut buf = String::new();
        render_stats(&stats, &mut buf);
        assert!(buf.contains("## 📊 Tipos de archivo"));
        assert!(buf.contains("| .pdf | 3 | 3.0 KB | 75.0 % |"));
        assert!(buf.contains(r"| .a\|b | 1 | 1.0 KB | 25.0 % |"));

        let mut empty = String::new();
        render_stats(&[], &mut empty);
        assert!(empty.is_empty());
    }
}
