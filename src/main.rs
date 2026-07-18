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
    if summary.skipped_count > 0 {
        println!(
            "⚠️  {} elementos no accesibles fueron omitidos",
            summary.skipped_count
        );
    }
    println!("⏱️  Tiempo: {:.2?}", start.elapsed());

    Ok(())
}

/// En Windows: oculta la ventana de consola cuando se lanza la GUI, pero
/// solo si esta consola fue creada exclusivamente para este proceso (caso
/// doble clic desde el Explorador). Si el usuario lanzó el .exe desde una
/// terminal interactiva (PowerShell/cmd), esa consola ya tenía dueño antes
/// (el propio shell) y ocultarla haría desaparecer la ventana del usuario.
/// Se distingue consultando cuántos procesos están adjuntos a la consola:
/// 1 → consola propia (doble clic); >1 → terminal interactiva ya existente.
#[cfg(windows)]
fn hide_console_window() {
    use std::ffi::c_void;
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
        fn GetConsoleProcessList(process_list: *mut u32, count: u32) -> u32;
    }
    const SW_HIDE: i32 = 0;
    unsafe {
        let hwnd = GetConsoleWindow();
        if hwnd.is_null() {
            return;
        }
        let mut pids = [0u32; 2];
        let attached = GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
        if attached <= 1 {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}
