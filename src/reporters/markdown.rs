use crate::models::Node;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn write_markdown(root: &Node, output: &Path) -> Result<()> {
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
