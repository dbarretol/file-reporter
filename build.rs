//! En Windows, incrusta en el ejecutable el icono de la aplicación (el que
//! muestra el Explorador) y los metadatos de versión.
//!
//! El `.ico` se genera aquí mismo con el dibujo de `src/icon.rs`, así que
//! el repositorio no guarda imágenes binarias y el icono del `.exe` coincide
//! siempre con el de la ventana.

#[path = "src/icon.rs"]
#[allow(dead_code)]
mod icon;

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");

    // Solo al compilar para Windows desde Windows (necesita el compilador de
    // recursos del SDK). En el resto de casos el ejecutable queda sin icono
    // incrustado, pero la ventana lo sigue mostrando.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_icon();
    }
}

#[cfg(windows)]
fn embed_windows_icon() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let ico_path = out_dir.join("app.ico");
    std::fs::write(&ico_path, ico(&[16, 20, 24, 32, 40, 48, 64, 256]))
        .expect("no se pudo escribir app.ico");

    // `winresource` también toma de Cargo.toml la versión y la descripción
    // que muestra "Propiedades → Detalles" del ejecutable.
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico_path.to_str().expect("ruta de OUT_DIR no UTF-8"));
    res.set("ProductName", "File Reporter");
    res.set("FileDescription", "File Reporter");
    res.compile()
        .expect("no se pudo incrustar el icono en el ejecutable");
}

/// Archivo `.ico` con una imagen por tamaño, cada una como mapa de bits de
/// 32 bits (BGRA, con alfa). Formato: cabecera `ICONDIR`, una entrada
/// `ICONDIRENTRY` por imagen y, después, los datos de cada una: cabecera
/// `BITMAPINFOHEADER`, píxeles de abajo arriba y máscara AND de 1 bit (toda
/// a cero: la transparencia la da el alfa).
#[cfg(windows)]
fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_entry(s)).collect();

    let mut out = Vec::new();
    out.extend(0u16.to_le_bytes()); // reservado
    out.extend(1u16.to_le_bytes()); // tipo: icono
    out.extend((sizes.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, data) in sizes.iter().zip(&images) {
        // 256 se escribe como 0 en los campos de un byte.
        let dim = if size >= 256 { 0 } else { size as u8 };
        out.extend([dim, dim, 0, 0]); // ancho, alto, colores de paleta, reservado
        out.extend(1u16.to_le_bytes()); // planos
        out.extend(32u16.to_le_bytes()); // bits por píxel
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for data in images {
        out.extend(data);
    }
    out
}

#[cfg(windows)]
fn bmp_entry(size: u32) -> Vec<u8> {
    let rgba = icon::rgba(size);
    // Cada fila de la máscara AND ocupa un múltiplo de 4 bytes.
    let mask_row = size.div_ceil(32) * 4;

    let mut out = Vec::new();
    out.extend(40u32.to_le_bytes()); // tamaño de BITMAPINFOHEADER
    out.extend((size as i32).to_le_bytes());
    out.extend((size as i32 * 2).to_le_bytes()); // alto x2: imagen + máscara
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend([0u8; 24]); // sin compresión, resto de campos a cero
    for row in rgba.chunks((size * 4) as usize).rev() {
        for p in row.chunks(4) {
            out.extend([p[2], p[1], p[0], p[3]]);
        }
    }
    out.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    out
}
