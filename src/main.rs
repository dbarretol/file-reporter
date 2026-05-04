use std::env;
use std::fs;
use std::path::PathBuf;

// Estructura para representar un archivo o directorio
#[derive(Debug)]
struct FileSystemNode {
    name: String,
    path: PathBuf,
    is_directory: bool,
    children: Vec<FileSystemNode>,
}

impl FileSystemNode {
    // Crear un nodo desde una ruta
    fn new(path: &PathBuf) -> Result<Self, std::io::Error> {
        let metadata = fs::metadata(path)?;
        let is_directory = metadata.is_dir();

        Ok(FileSystemNode {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.to_path_buf(),
            is_directory,
            children: Vec::new(),
        })
    }

    // Recorrer recursivamente el directorio
    fn scan(&mut self) -> Result<(), std::io::Error> {
        if self.is_directory {
            for entry in fs::read_dir(&self.path)? {
                let entry = entry?;
                let entry_path = entry.path();
                let mut child = FileSystemNode::new(&entry_path)?;
                if child.is_directory {
                    child.scan()?;
                }
                self.children.push(child);
            }
        }
        Ok(())
    }

    // Generar el reporte en formato TXT
    fn generate_txt_report(&self, report: &mut String) {
        self.generate_txt_recursive(report, 0);
    }

    // Generar el reporte en formato Markdown
    fn generate_markdown_report(&self, report: &mut String) {
        self.generate_markdown_recursive(report, 0);
    }

    // Generar el reporte en formato HTML
    fn generate_html_report(&self, report: &mut String) {
        self.generate_html_recursive(report, 0);
    }

    // Implementación recursiva para TXT
    fn generate_txt_recursive(&self, report: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        let icon = if self.is_directory { "[D] " } else { "[F] " };

        report.push_str(&format!("{}{}{}\n", indent, icon, self.name));

        for child in &self.children {
            child.generate_txt_recursive(report, depth + 1);
        }
    }

    // Implementación recursiva para Markdown
    fn generate_markdown_recursive(&self, report: &mut String, depth: usize) {
        let icon = if self.is_directory { "📂" } else { "📄" };

        // Agregar sangría basada en la profundidad
        report.push_str(&"  ".repeat(depth));
        report.push_str(&format!("{} {}\\n", icon, self.name));

        for child in &self.children {
            child.generate_markdown_recursive(report, depth + 1);
        }
    }

    // Implementación recursiva para HTML
    fn generate_html_recursive(&self, report: &mut String, depth: usize) {
        let icon = if self.is_directory { "📁" } else { "📄" };

        // Agregar sangría usando estilos CSS
        let style = format!("padding-left: {}px;", depth * 20);
        report.push_str(&format!(
            "<div style='{}'>{} {}</div>\\n",
            style,
            icon,
            self.name
        ));

        for child in &self.children {
            child.generate_html_recursive(report, depth + 1);
        }
    }
}

fn print_usage() {
    println!("\\nUso:");
    println!("  full-path-reporter <directorio> [output_path]\\n");
    println!("Ejemplos:");
    println!("  full-path-reporter C:\\Users");
    println!("  full-path-reporter . ./reportes\\n");
    println!("Formatos de salida:");
    println!("  TXT: report.txt");
    println!("  Markdown: report.md");
    println!("  HTML: report.html");
}

fn main() {
    // Obtener argumentos de la línea de comandos
    let args: Vec<String> = env::args().collect();

    // Verificar que se haya proporcionado al menos un argumento (directorio)
    if args.len() < 2 {
        eprintln!("❌ Error: Debes especificar el directorio a escanear.");
        print_usage();
        return;
    }

    // Obtener la ruta del directorio
    let directory_path = PathBuf::from(&args[1]);

    // Determinar la ruta de salida
    let output_base_path = if args.len() > 2 {
        PathBuf::from(&args[2])
    } else {
        // Si no se proporciona, usar el directorio actual
        env::current_dir().unwrap_or_default()
    };

    println!("🔍 Escaneando directorio: {}\\n", directory_path.display());

    // Crear el nodo raíz
    let mut root = match FileSystemNode::new(&directory_path) {
        Ok(node) => node,
        Err(err) => {
            eprintln!("❌ Error al crear el nodo raíz: {}\\n", err);
            return;
        }
    };

    // Escanear recursivamente
    if let Err(err) = root.scan() {
        eprintln!("❌ Error al escanear el directorio: {}\\n", err);
        return;
    }

    // Generar reportes en todos los formatos
    let formats = ["txt", "md", "html"];

    for format in formats {
        let mut report_content = String::new();

        // Agregar encabezados según el formato
        match format {
            "txt" => {
                report_content.push_str(&format!("REPORTE DE ESTRUCTURA DE DIRECTORIOS\\n"));
                report_content.push_str(&format!("{}\\n", "=".repeat(60)));
                report_content.push_str(&format!("🔍 Directorio: {}\\n", directory_path.display()));
                report_content.push_str(&format!("{}\\n\\n", "=".repeat(60)));
                root.generate_txt_report(&mut report_content);
            }
            "md" => {
                report_content.push_str(&format!("# 📋 Estructura de Directorios\\n\\n"));
                report_content.push_str(&format!("**🔍 Directorio:** `{}`\\n\\n", directory_path.display()));
                root.generate_markdown_report(&mut report_content);
            }
            "html" => {
                report_content.push_str(
                    "<!DOCTYPE html><html lang='es'><head><meta charset='UTF-8'>"
                );
                report_content.push_str(
                    "<title>Reporte de Estructura de Directorios</title>"
                );
                report_content.push_str(
                    "<style>body{font-family:sans-serif;padding:20px;background-color:#f5f5f5;}</style>"
                );
                report_content.push_str("</head><body>");
                report_content.push_str("<h1>📋 Estructura de Directorios</h1>");
                report_content.push_str(&format!("<p><strong>🔍 Directorio:</strong> {}</p>", directory_path.display()));
                root.generate_html_report(&mut report_content);
                report_content.push_str("</body></html>");
            }
            _ => unreachable!(),
        }

        // Construir la ruta del archivo
