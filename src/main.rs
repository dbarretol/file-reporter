// Importante: NO usar #![windows_subsystem = "windows"] global,
// porque queremos que la CLI siga mostrando salida en consola.

mod cli;
mod gui;
mod models;
mod reporters;
mod runner;
mod scanner;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use std::time::Instant;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Sin argumentos → lanzar GUI
    if args.len() <= 1 {
        #[cfg(windows)]
        hide_console_window();
        return gui::launch();
    }

    // Con argumentos → modo CLI (igual que antes)
    let parsed = Cli::parse();

    let input = parsed.path.clone();
    let output = parsed.output.clone().unwrap_or_else(|| input.clone());
    let extensions = parsed.normalized_extensions();

    if !extensions.is_empty() {
        println!("🎯 Filtrando extensiones: {:?}", extensions);
    }
    println!("🔍 Escaneando: {}", input.display());

    let start = Instant::now();
    let summary = runner::run(
        &runner::RunConfig {
            input,
            output,
            extensions,
        },
        None,
    )?;

    println!("\n✅ Reportes generados correctamente:");
    println!("   📄 CSV:      {}", summary.csv_path.display());
    println!("   📄 Markdown: {}", summary.md_path.display());
    println!("   📄 JSON:     {}", summary.json_path.display());
    println!("\n📊 Archivos procesados: {}", summary.files_count);
    println!("⏱️  Tiempo: {:.2?}", start.elapsed());

    Ok(())
}

/// En Windows: oculta la ventana de consola cuando se lanza la GUI.
/// En CLI no se llama, por lo que la consola sigue funcionando normal.
#[cfg(windows)]
fn hide_console_window() {
    use std::ffi::c_void;
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
    }
    const SW_HIDE: i32 = 0;
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}
