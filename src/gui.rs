use crate::runner::{self, Progress, RunConfig, RunSummary};
use anyhow::Result;
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Instant;

pub fn launch() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 460.0])
            .with_min_inner_size([520.0, 380.0])
            .with_title("File Reporter"),
        ..Default::default()
    };

    eframe::run_native(
        "File Reporter",
        options,
        Box::new(|_cc| Ok(Box::<App>::default())),
    )
    .map_err(|e| anyhow::anyhow!("Error iniciando GUI: {e}"))
}

#[derive(Default)]
struct App {
    input_path: String,
    output_path: String,
    extensions: String,
    status: Status,

    // Comunicación con el worker
    progress: Option<Arc<Progress>>,
    rx: Option<Receiver<WorkerMsg>>,
    started_at: Option<Instant>,
    summary: Option<RunSummary>,
    error: Option<String>,
}

#[derive(Default, PartialEq)]
enum Status {
    #[default]
    Idle,
    Running,
    Done,
    Failed,
}

enum WorkerMsg {
    Done(RunSummary),
    Failed(String),
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pedir repintado constante mientras se ejecuta (para refrescar progreso/ETA)
        if self.status == Status::Running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Drenar mensajes del worker
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMsg::Done(s) => {
                        self.summary = Some(s);
                        self.status = Status::Done;
                    }
                    WorkerMsg::Failed(e) => {
                        self.error = Some(e);
                        self.status = Status::Failed;
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("📋 File Reporter");
            ui.label("Genera reportes (CSV, Markdown, JSON) de la estructura de un directorio.");
            ui.separator();

            // ─── Carpeta de entrada ──────────────────────────────
            ui.label("📁 Carpeta a escanear:");
            ui.horizontal(|ui| {
                let enabled = self.status != Status::Running;
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut self.input_path)
                        .hint_text("C:\\ruta\\a\\la\\carpeta")
                        .desired_width(450.0),
                );
                if ui
                    .add_enabled(enabled, egui::Button::new("Examinar…"))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.input_path = path.display().to_string();
                        if self.output_path.is_empty() {
                            self.output_path = self.input_path.clone();
                        }
                    }
                }
            });

            ui.add_space(8.0);

            // ─── Carpeta de salida ───────────────────────────────
            ui.label("💾 Carpeta de salida (opcional):");
            ui.horizontal(|ui| {
                let enabled = self.status != Status::Running;
                ui.add_enabled(
                    enabled,
                    egui::TextEdit::singleline(&mut self.output_path)
                        .hint_text("Por defecto: misma que la entrada")
                        .desired_width(450.0),
                );
                if ui
                    .add_enabled(enabled, egui::Button::new("Examinar…"))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.output_path = path.display().to_string();
                    }
                }
            });

            ui.add_space(8.0);

            // ─── Extensiones ─────────────────────────────────────
            ui.label("🎯 Extensiones (separadas por coma, vacío = todas):");
            ui.add_enabled(
                self.status != Status::Running,
                egui::TextEdit::singleline(&mut self.extensions)
                    .hint_text("pdf, docx, jpg")
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(12.0);
            ui.separator();

            // ─── Botón de ejecución ──────────────────────────────
            ui.horizontal(|ui| {
                let can_run = self.status != Status::Running && !self.input_path.trim().is_empty();
                if ui
                    .add_enabled(
                        can_run,
                        egui::Button::new("▶  Generar reportes").min_size(egui::vec2(180.0, 32.0)),
                    )
                    .clicked()
                {
                    self.start_run();
                }

                if self.status == Status::Running {
                    ui.spinner();
                    ui.label("Procesando…");
                }
            });

            ui.add_space(10.0);

            // ─── Barra de progreso + ETA ─────────────────────────
            if let Some(p) = &self.progress {
                let frac = p.fraction();
                let done = p.done.load(Ordering::Relaxed);
                let total = p.total.load(Ordering::Relaxed);

                let eta_text = self.compute_eta(frac);
                let label = if total > 0 {
                    format!("{} / {} archivos  •  {}", done, total, eta_text)
                } else {
                    "Contando archivos…".to_string()
                };

                ui.add(
                    egui::ProgressBar::new(frac)
                        .show_percentage()
                        .animate(self.status == Status::Running),
                );
                ui.label(label);
            }

            ui.add_space(10.0);

            // ─── Resultados / errores ────────────────────────────
            match self.status {
                Status::Done => {
                    if let Some(s) = &self.summary {
                        ui.colored_label(
                            egui::Color32::from_rgb(0, 160, 0),
                            "✅ Reportes generados:",
                        );
                        ui.monospace(format!("• CSV : {}", s.csv_path.display()));
                        ui.monospace(format!("• MD  : {}", s.md_path.display()));
                        ui.monospace(format!("• JSON: {}", s.json_path.display()));
                        ui.label(format!("Total de archivos procesados: {}", s.files_count));

                        if ui.button("📂 Abrir carpeta de salida").clicked() {
                            if let Some(parent) = s.csv_path.parent() {
                                let _ = open_in_explorer(parent);
                            }
                        }
                    }
                }
                Status::Failed => {
                    if let Some(e) = &self.error {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 0, 0),
                            format!("❌ Error: {e}"),
                        );
                    }
                }
                _ => {}
            }
        });
    }
}

impl App {
    fn start_run(&mut self) {
        // Reset de estado
        self.summary = None;
        self.error = None;
        self.status = Status::Running;
        self.started_at = Some(Instant::now());

        let progress = Progress::new();
        self.progress = Some(progress.clone());

        let (tx, rx) = mpsc::channel::<WorkerMsg>();
        self.rx = Some(rx);

        let input = PathBuf::from(self.input_path.trim());
        let output = if self.output_path.trim().is_empty() {
            input.clone()
        } else {
            PathBuf::from(self.output_path.trim())
        };
        let extensions: Vec<String> = self
            .extensions
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .map(|s| s.trim().trim_start_matches('.').to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let cfg = RunConfig {
            input,
            output,
            extensions,
        };

        // Worker en hilo separado para no bloquear la GUI
        std::thread::spawn(move || {
            let result = runner::run(&cfg, Some(progress));
            let msg = match result {
                Ok(s) => WorkerMsg::Done(s),
                Err(e) => WorkerMsg::Failed(e.to_string()),
            };
            let _ = tx.send(msg);
        });
    }

    fn compute_eta(&self, fraction: f32) -> String {
        let Some(start) = self.started_at else {
            return "ETA: --".into();
        };
        if fraction <= 0.001 {
            return "ETA: calculando…".into();
        }
        if fraction >= 1.0 || self.status == Status::Done {
            let elapsed = start.elapsed().as_secs_f32();
            return format!("Tiempo total: {}", fmt_seconds(elapsed));
        }
        let elapsed = start.elapsed().as_secs_f32();
        let total_est = elapsed / fraction;
        let remaining = (total_est - elapsed).max(0.0);
        format!("ETA: {}", fmt_seconds(remaining))
    }
}

fn fmt_seconds(s: f32) -> String {
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
fn open_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn open_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    std::process::Command::new(cmd)
        .arg(path)
        .spawn()
        .map(|_| ())
}
