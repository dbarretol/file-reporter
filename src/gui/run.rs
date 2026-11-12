//! Estado de la corrida y comunicación con el hilo que escanea.

use super::util::clean_path_input;
use super::App;
use crate::runner::{self, Progress, RunConfig, RunSummary};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// En qué punto está la corrida. Cada variante lleva solo los datos que
/// tienen sentido en ella (p. ej. no hay resumen sin corrida terminada).
#[derive(Default)]
pub(super) enum RunState {
    #[default]
    Idle,
    Running(ActiveRun),
    Done {
        summary: RunSummary,
        elapsed: Duration,
    },
    Failed {
        error: String,
    },
    /// El usuario pidió cancelar y el worker terminó en respuesta a eso.
    /// Distinto de `Failed`: no es un error, así que la UI no debe mostrar
    /// el panel rojo de error.
    Cancelled {
        /// Elementos encontrados antes de detenerse.
        found: usize,
        dirs_only: bool,
        elapsed: Duration,
    },
}

/// Corrida en curso.
pub(super) struct ActiveRun {
    pub progress: Arc<Progress>,
    rx: Receiver<WorkerMsg>,
    pub started: Instant,
    /// Modo de esta corrida (la casilla puede cambiar cuando termine).
    pub dirs_only: bool,
}

impl ActiveRun {
    /// `true` una vez terminado el escaneo, mientras se escriben los reportes.
    pub fn writing(&self) -> bool {
        self.progress.writing.load(Ordering::Relaxed)
    }

    pub fn found(&self) -> usize {
        self.progress.done.load(Ordering::Relaxed)
    }

    /// Qué se cuenta en esta corrida, para los textos de progreso.
    pub fn noun(&self) -> &'static str {
        runner::found_noun(self.dirs_only)
    }
}

/// Corrida a la espera de que el usuario confirme la sobrescritura.
pub(super) struct PendingRun {
    pub cfg: RunConfig,
    /// Reportes existentes que pisaría (calculados una sola vez).
    pub existing: Vec<PathBuf>,
}

enum WorkerMsg {
    Done(RunSummary),
    Failed(String),
    Cancelled,
}

impl App {
    pub(super) fn is_running(&self) -> bool {
        matches!(self.state, RunState::Running(_))
    }

    /// Hay un escaneo en curso o un diálogo de confirmación abierto: el
    /// formulario queda inerte. El diálogo no es un modal real de egui, así
    /// que sin esto se podría seguir editando "por debajo" de él.
    pub(super) fn is_busy(&self) -> bool {
        self.is_running() || self.pending_confirm.is_some()
    }

    /// Arma el `RunConfig` a partir de los campos del formulario.
    fn build_cfg(&self) -> RunConfig {
        // "Copiar como ruta de acceso" de Windows pega la ruta entre comillas
        // (p. ej. "C:\..."); las quitamos además del trim de espacios normal.
        let input = PathBuf::from(clean_path_input(&self.input_path));
        let output = if clean_path_input(&self.output_path).is_empty() {
            input.clone()
        } else {
            PathBuf::from(clean_path_input(&self.output_path))
        };
        // El texto de extensiones se conserva en el formulario, pero en modo
        // solo carpetas no se envía (igual que la CLI, que rechaza `-d -e`).
        let extensions: Vec<String> = if self.dirs_only {
            Vec::new()
        } else {
            runner::parse_extensions([self.extensions.as_str()])
        };

        RunConfig {
            input,
            output,
            extensions,
            dirs_only: self.dirs_only,
            exclude_dirs: runner::parse_dir_names([self.exclude_dirs.as_str()]),
            formats: self.selected_formats(),
        }
    }

    /// Se llama al pulsar "Generar reportes". Si ya existen reportes de una
    /// corrida anterior en la carpeta de salida, pide confirmación antes de
    /// sobrescribirlos en vez de pisarlos en silencio.
    pub(super) fn request_run(&mut self) {
        // La carpeta pudo cambiar en disco sin que cambiara el texto: que la
        // validación en vivo vuelva a mirar el disco en el próximo frame.
        self.input_check.invalidate();
        self.output_check.invalidate();
        let cfg = self.build_cfg();
        // Se calcula una sola vez aquí, no en cada frame mientras el diálogo
        // de confirmación está abierto.
        let existing = existing_reports(&cfg);
        if existing.is_empty() {
            self.start_run(cfg);
        } else {
            self.pending_confirm = Some(PendingRun { cfg, existing });
        }
    }

    pub(super) fn start_run(&mut self, cfg: RunConfig) {
        self.copied = None;
        self.clear_ui_error();

        let progress = Progress::new();
        let (tx, rx) = mpsc::channel::<WorkerMsg>();
        let dirs_only = cfg.dirs_only;
        let worker_progress = progress.clone();

        // Worker en hilo separado. Capturamos panics para que nunca tumben la GUI.
        // Pila ampliada (ver `main::SCAN_THREAD_STACK_SIZE`): este hilo inicia la
        // recursión de `scanner::build_tree` y, por el work-stealing de rayon,
        // también puede ejecutar parte de ella directamente en su propia pila.
        let spawn_result = std::thread::Builder::new()
            .stack_size(crate::SCAN_THREAD_STACK_SIZE)
            .spawn(move || {
                let progress = worker_progress;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runner::run(&cfg, Some(progress.clone()))
                }));

                let msg = match result {
                    Ok(Ok(s)) => WorkerMsg::Done(s),
                    // Distinguimos cancelación de error real preguntando la
                    // bandera compartida (la puso en `true` el botón
                    // "Cancelar"), no comparando el texto del error: así no
                    // depende de que el mensaje de `runner::run` no cambie.
                    Ok(Err(e)) => {
                        if progress.cancel.load(Ordering::Relaxed) {
                            WorkerMsg::Cancelled
                        } else {
                            WorkerMsg::Failed(error_chain(&e))
                        }
                    }
                    Err(panic_payload) => {
                        let detail = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "panic desconocido en el worker".to_string()
                        };
                        WorkerMsg::Failed(format!("Error inesperado: {detail}"))
                    }
                };
                let _ = tx.send(msg);
            });

        self.state = match spawn_result {
            Ok(_) => RunState::Running(ActiveRun {
                progress,
                rx,
                started: Instant::now(),
                dirs_only,
            }),
            Err(_) => {
                self.set_ui_error("No se pudo iniciar el hilo de escaneo");
                RunState::Idle
            }
        };
    }

    pub(super) fn request_cancel(&self) {
        if let RunState::Running(run) = &self.state {
            run.progress.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Recoge el resultado del worker, si ya terminó.
    pub(super) fn poll_worker(&mut self, ctx: &egui::Context) {
        let RunState::Running(run) = &self.state else {
            return;
        };
        let Ok(msg) = run.rx.try_recv() else {
            return;
        };
        let elapsed = run.started.elapsed();
        // Si el usuario se fue a otra ventana durante un escaneo largo, la
        // de la app parpadea en la barra de tareas al terminar. Cancelar fue
        // una acción suya: no hace falta avisar.
        let (next, attention) = match msg {
            WorkerMsg::Done(summary) => (
                RunState::Done { summary, elapsed },
                Some(egui::UserAttentionType::Informational),
            ),
            WorkerMsg::Failed(error) => (
                RunState::Failed { error },
                Some(egui::UserAttentionType::Critical),
            ),
            WorkerMsg::Cancelled => (
                RunState::Cancelled {
                    found: run.found(),
                    dirs_only: run.dirs_only,
                    elapsed,
                },
                None,
            ),
        };
        self.state = next;
        // La corrida pudo crear la carpeta de salida: que el aviso "se
        // creará al generar los reportes" no quede desactualizado.
        self.output_check.invalidate();
        self.results_open = true;
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if let (Some(kind), false) = (attention, focused) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(kind));
        }
    }
}

/// Mensaje de error completo para mostrar al usuario: el contexto y todas
/// sus causas (`contexto: causa: …`). `to_string()` de anyhow solo da el
/// contexto exterior ("Error al construir el árbol…"), sin decir por qué.
fn error_chain(e: &anyhow::Error) -> String {
    format!("{e:#}")
}

/// Reportes de una corrida anterior que ya existen en la carpeta de salida
/// de `cfg` y que esta corrida sobrescribiría (solo los de los formatos
/// elegidos: los demás no se tocan).
fn existing_reports(cfg: &RunConfig) -> Vec<PathBuf> {
    cfg.report_paths()
        .into_iter()
        .map(|(_, p)| p)
        .filter(|p| p.exists())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_chain_includes_the_root_cause() {
        use anyhow::Context;
        let err = std::fs::read_dir("no-existe-esta-carpeta-xyz")
            .context("Error al construir el árbol de directorios")
            .unwrap_err();
        let msg = error_chain(&err);
        assert!(msg.starts_with("Error al construir el árbol de directorios: "));
        assert!(msg.len() > "Error al construir el árbol de directorios: ".len());
    }

    /// App con una corrida en curso (sin hilo real) y el emisor del canal.
    fn running_app(dirs_only: bool) -> (App, mpsc::Sender<WorkerMsg>, Arc<Progress>) {
        let (tx, rx) = mpsc::channel();
        let progress = Progress::new();
        let app = App {
            state: RunState::Running(ActiveRun {
                progress: progress.clone(),
                rx,
                started: Instant::now(),
                dirs_only,
            }),
            ..App::default()
        };
        (app, tx, progress)
    }

    #[test]
    fn poll_worker_waits_while_the_worker_runs() {
        let (mut app, _tx, _) = running_app(false);
        app.poll_worker(&egui::Context::default());
        assert!(app.is_running());
        assert!(!app.results_open);
    }

    #[test]
    fn poll_worker_keeps_the_failure_message() {
        let (mut app, tx, _) = running_app(false);
        tx.send(WorkerMsg::Failed("sin permiso".into())).unwrap();
        app.poll_worker(&egui::Context::default());
        assert!(matches!(&app.state, RunState::Failed { error } if error == "sin permiso"));
        assert!(app.results_open);
        assert!(app.results_visible());
    }

    #[test]
    fn results_modal_is_hidden_without_a_finished_run() {
        // `results_open` solo no basta: sin resultado (Idle o en curso) no
        // hay modal, y Enter vuelve a lanzar corridas.
        let (mut app, _tx, _) = running_app(false);
        app.results_open = true;
        assert!(!app.results_visible());
        app.state = RunState::Idle;
        assert!(!app.results_visible());
    }

    #[test]
    fn cancelled_run_keeps_its_own_count_and_mode() {
        // El panel de cancelado describe la corrida que se canceló, aunque
        // después cambie la casilla "Solo carpetas".
        let (mut app, tx, progress) = running_app(true);
        progress.done.store(7, Ordering::Relaxed);
        tx.send(WorkerMsg::Cancelled).unwrap();
        app.poll_worker(&egui::Context::default());
        app.dirs_only = false;
        assert!(matches!(
            app.state,
            RunState::Cancelled {
                found: 7,
                dirs_only: true,
                ..
            }
        ));
    }

    #[test]
    fn request_cancel_sets_the_shared_flag() {
        let (app, _tx, progress) = running_app(false);
        app.request_cancel();
        assert!(progress.cancel.load(Ordering::Relaxed));
    }
}
