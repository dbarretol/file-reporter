//! Colores de la interfaz, con una variante para cada tema, y la elección
//! de tema del usuario.

use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};

/// Colores semánticos de la interfaz para el tema activo.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub accent: Color32,
    pub success: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub muted: Color32,
    /// Fondo del panel de error.
    pub error_fill: Color32,
}

impl Palette {
    /// Tonos pensados para tema oscuro (el diseño original).
    const DARK: Self = Self {
        accent: Color32::from_rgb(70, 130, 220),
        success: Color32::from_rgb(46, 160, 67),
        error: Color32::from_rgb(220, 70, 70),
        warning: Color32::from_rgb(210, 150, 40),
        muted: Color32::from_rgb(150, 150, 150),
        error_fill: Color32::from_rgba_unmultiplied_const(220, 70, 70, 20),
    };

    /// Variantes más oscuras/saturadas (~Tailwind 600-800) para tema claro:
    /// los tonos de `DARK`, calibrados para fondo oscuro, quedan por debajo
    /// de 4.5:1 de contraste (WCAG AA para texto) sobre fondo blanco.
    const LIGHT: Self = Self {
        accent: Color32::from_rgb(30, 64, 175),
        success: Color32::from_rgb(21, 128, 61),
        error: Color32::from_rgb(185, 28, 28),
        warning: Color32::from_rgb(180, 83, 9),
        muted: Color32::from_rgb(75, 85, 99),
        error_fill: Color32::from_rgba_unmultiplied_const(220, 70, 70, 20),
    };

    /// Paleta del tema activo (el elegido o, por defecto, el del sistema). Toma un
    /// `Ui` porque cada sitio de uso ya tiene uno a mano.
    pub fn of(ui: &egui::Ui) -> Self {
        if ui.visuals().dark_mode {
            Self::DARK
        } else {
            Self::LIGHT
        }
    }
}

/// Tema elegido en la cabecera. Tipo propio (y no `egui::ThemePreference`)
/// para que el estado guardado no dependa de cómo egui serialice el suyo.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum ThemeChoice {
    /// Seguir el tema claro/oscuro del sistema operativo.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Sistema",
            Self::Light => "Claro",
            Self::Dark => "Oscuro",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::System => "Usar el tema claro u oscuro del sistema operativo",
            Self::Light => "Usar siempre el tema claro",
            Self::Dark => "Usar siempre el tema oscuro",
        }
    }

    /// Aplica el tema a la interfaz y también a la barra de título, que
    /// dibuja el sistema operativo: sin el `SetTheme`, en Windows sus botones
    /// (minimizar, maximizar, cerrar) quedaban con los colores del otro tema
    /// y apenas se veían.
    pub fn apply(self, ctx: &egui::Context) {
        ctx.set_theme(self.to_egui());
        self.apply_native(ctx);
    }

    /// Solo la barra de título (ver `apply`).
    pub fn apply_native(self, ctx: &egui::Context) {
        let native = match self {
            Self::System => egui::SystemTheme::SystemDefault,
            Self::Light => egui::SystemTheme::Light,
            Self::Dark => egui::SystemTheme::Dark,
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(native));
    }

    fn to_egui(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Light => egui::ThemePreference::Light,
            Self::Dark => egui::ThemePreference::Dark,
        }
    }
}
