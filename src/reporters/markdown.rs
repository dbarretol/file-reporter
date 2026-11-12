use crate::models::{ExtStat, Node};
use crate::summary::{format_percent, format_size};
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Escribe el árbol de directorios y, si hay archivos, una tabla con los
/// tipos de archivo (`stats`, ya ordenada de mayor a menor tamaño).
///
/// Se escribe línea a línea a medida que se recorre el árbol (como el CSV),
/// sin armar antes el documento entero en memoria: con cientos de miles de
/// archivos eran decenas de MB extra. Va a través de un temporal que solo
/// reemplaza al reporte anterior si todo fue bien (ver
/// `reporters::write_atomic`).
pub fn write_markdown(root: &Node, stats: &[ExtStat], output: &Path) -> Result<()> {
    let root_line = match root {
        Node::Directory { path, .. } | Node::File { path, .. } => path.display().to_string(),
    };
    // La valla depende de todo el contenido del bloque: se calcula antes de
    // escribir, con una pasada en memoria por los nombres.
    let longest = longest_backtick_run_in_tree(root).max(backtick_run(&root_line));
    let fence = "`".repeat(longest.max(2) + 1);

    super::write_atomic(output, "Markdown", |w| {
        (|| -> std::io::Result<()> {
            write!(
                w,
                "# 📋 Estructura de Directorios\n\n{fence}\n{root_line}\n"
            )?;
            if let Node::Directory { children, .. } = root {
                render_children(children, &mut String::new(), w)?;
            }
            writeln!(w, "{fence}")?;
            render_stats(stats, w)
        })()
        .with_context(|| format!("No se pudo escribir Markdown en {}", output.display()))
    })
}

/// Racha más larga de acentos graves seguidos en `s`.
fn backtick_run(s: &str) -> usize {
    s.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// Racha más larga de acentos graves en los nombres del árbol. El bloque del
/// árbol usa una valla de tres acentos graves o, si algún nombre (válido en
/// Windows y Linux) tiene una racha así, una más que la más larga, para que
/// ese nombre no cierre el bloque antes de tiempo (CommonMark).
fn longest_backtick_run_in_tree(node: &Node) -> usize {
    match node {
        Node::Directory { children, .. } => children
            .iter()
            .map(|c| backtick_run(c.name()).max(longest_backtick_run_in_tree(c)))
            .max()
            .unwrap_or(0),
        Node::File { .. } => 0,
    }
}

/// Una línea por hijo, con las ramas de `tree`. `prefix` se reutiliza en
/// toda la recursión (se alarga al bajar un nivel y se recorta al volver),
/// en vez de crear una cadena nueva por carpeta.
fn render_children(
    children: &[Node],
    prefix: &mut String,
    w: &mut impl Write,
) -> std::io::Result<()> {
    let total = children.len();
    for (i, child) in children.iter().enumerate() {
        let is_last = i + 1 == total;
        let connector = if is_last { "└── " } else { "├── " };
        writeln!(w, "{prefix}{connector}{}", child.name())?;

        if let Node::Directory { children, .. } = child {
            let len = prefix.len();
            prefix.push_str(if is_last { "    " } else { "│   " });
            render_children(children, prefix, w)?;
            prefix.truncate(len);
        }
    }
    Ok(())
}

fn render_stats(stats: &[ExtStat], w: &mut impl Write) -> std::io::Result<()> {
    if stats.is_empty() {
        return Ok(());
    }
    let total: u64 = stats.iter().map(|s| s.bytes).sum();
    w.write_all(
        "\n## 📊 Tipos de archivo\n\n| Extensión | Archivos | Tamaño | % del tamaño |\n|---|--:|--:|--:|\n"
            .as_bytes(),
    )?;
    for s in stats {
        // Un `|` en la extensión (posible en Linux) rompería la tabla.
        let label = s.label().replace('|', r"\|");
        writeln!(
            w,
            "| {label} | {} | {} | {} |",
            s.count,
            format_size(s.bytes),
            format_percent(s.bytes, total)
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backtick_run_finds_the_longest_streak() {
        assert_eq!(backtick_run("a"), 0);
        assert_eq!(backtick_run("un `nombre`"), 1);
        assert_eq!(backtick_run("raro```.txt"), 3);
        assert_eq!(backtick_run("x`````y``"), 5);
    }

    #[test]
    fn writes_the_tree_with_branches() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        std::fs::create_dir_all(input.join("a").join("b")).unwrap();
        std::fs::write(input.join("a").join("b").join("x.txt"), "x").unwrap();
        std::fs::write(input.join("a").join("y.txt"), "x").unwrap();
        std::fs::write(input.join("z.txt"), "x").unwrap();
        let tree =
            crate::scanner::build_tree(&input, &crate::scanner::ScanOptions::default()).unwrap();

        let output = dir.path().join("report.md");
        write_markdown(&tree, &[], &output).unwrap();

        let md = std::fs::read_to_string(&output).unwrap();
        let body: Vec<&str> = md.lines().skip(4).take(5).collect();
        assert_eq!(
            body,
            vec![
                "├── a",
                "│   ├── b",
                "│   │   └── x.txt",
                "│   └── y.txt",
                "└── z.txt",
            ]
        );
        assert_eq!(md.lines().nth(9), Some("```"));
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
        let mut out = Vec::new();
        render_stats(&stats, &mut out).unwrap();
        let buf = String::from_utf8(out).unwrap();
        assert!(buf.contains("## 📊 Tipos de archivo"));
        assert!(buf.contains("| .pdf | 3 | 3.0 KB | 75.0 % |"));
        assert!(buf.contains(r"| .a\|b | 1 | 1.0 KB | 25.0 % |"));

        let mut empty = Vec::new();
        render_stats(&[], &mut empty).unwrap();
        assert!(empty.is_empty());
    }
}
