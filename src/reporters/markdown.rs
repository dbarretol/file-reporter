use crate::models::{ExtStat, Node};
use crate::runner::{format_percent, format_size};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Escribe el árbol de directorios y, si hay archivos, una tabla con los
/// tipos de archivo (`stats`, ya ordenada de mayor a menor tamaño).
pub fn write_markdown(root: &Node, stats: &[ExtStat], output: &Path) -> Result<()> {
    let mut buf = String::new();
    buf.push_str("# 📋 Estructura de Directorios\n\n");
    buf.push_str("```\n");

    // Raíz
    match root {
        Node::Directory { path, children, .. } => {
            buf.push_str(&format!("{}\n", path.display()));
            render_children(children, "", &mut buf);
        }
        Node::File { path, .. } => {
            buf.push_str(&format!("{}\n", path.display()));
        }
    }

    buf.push_str("```\n");
    render_stats(stats, &mut buf);
    fs::write(output, buf)
        .with_context(|| format!("No se pudo escribir Markdown en {}", output.display()))?;
    Ok(())
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
