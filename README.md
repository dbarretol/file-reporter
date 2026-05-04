# 📋 File Reporter

> Herramienta multiplataforma escrita en **Rust** que escanea recursivamente un directorio y genera reportes en **CSV, Markdown y JSON** sobre su estructura y contenido.

Diseñada para ser **rápida**, **robusta** y **fácil de usar**, ofrece tanto una interfaz gráfica (GUI) amigable como una potente interfaz de línea de comandos (CLI) para integrarse en flujos de trabajo automatizados.

---

## ✨ Características principales

- 🚀 **Alto rendimiento** — escaneo paralelo con [`rayon`](https://crates.io/crates/rayon) para grandes volúmenes de archivos.
- 🖥️ **Doble interfaz** — GUI nativa (con [`egui`](https://github.com/emilk/egui)) o línea de comandos (con [`clap`](https://crates.io/crates/clap)).
- 📊 **Tres formatos de salida**:
  - **CSV** compatible con Excel (UTF-8 con BOM, separador `;`).
  - **Markdown** con árbol jerárquico estilo `tree`.
  - **JSON** estructurado para integraciones.
- 🎯 **Filtro por extensiones** — analiza solo los tipos de archivo que te interesan.
- 📅 **Metadatos completos** — nombre, extensión, tamaño en MB, fechas de creación y modificación, ruta.
- 🛡️ **Manejo robusto de errores** — sigue funcionando aunque haya carpetas sin permisos.
- 📈 **Barra de progreso con ETA** en la GUI.
- 🪟 **Multiplataforma** — Windows, Linux y macOS (probado principalmente en Windows).

---

## 🖼️ Capturas

> *(Añade aquí tus screenshots de la GUI cuando los tengas. Por ejemplo:)*
>
> ![GUI principal](docs/screenshot-main.png)
> ![Reportes generados](docs/screenshot-results.png)

---

## 📦 Requisitos

- **Rust** 1.76 o superior — instalable desde [rustup.rs](https://rustup.rs/).
- **Linux únicamente:** dependencias del sistema para diálogos nativos:
  ```bash
  sudo apt install libgtk-3-dev libxkbcommon-dev
  ```

---

## 🛠️ Compilación

Clona el repositorio y compila en modo **release** para máximo rendimiento:

```bash
git clone https://github.com/tu-usuario/file-reporter.git
cd file-reporter
cargo build --release
```

El ejecutable se generará en `target/release/file-reporter` (o `file-reporter.exe` en Windows).

---

## 🚀 Uso

### Modo GUI (gráfico)

Simplemente ejecuta el binario sin argumentos, o haz **doble clic** sobre el `.exe` en Windows:

```bash
./target/release/file-reporter
```

Aparecerá una ventana donde podrás:

1. Seleccionar la carpeta a escanear (escribiendo la ruta o usando "Examinar…").
2. Definir una carpeta de salida personalizada (opcional).
3. Especificar extensiones a filtrar, con presets rápidos.
4. Pulsar **▶ Generar reportes** y monitorizar el progreso en tiempo real.

### Modo CLI (línea de comandos)

```bash
# Escanear todo el contenido de un directorio
./target/release/file-reporter /ruta/a/escanear

# Filtrar por extensiones (varias formas)
./target/release/file-reporter /ruta -e txt -e jpg -e pdf
./target/release/file-reporter /ruta --ext txt,jpg,png

# Especificar carpeta de salida personalizada
./target/release/file-reporter /ruta -e mp4 -o /ruta/reportes

# En Windows con PowerShell
.\target\release\file-reporter.exe "D:\MisDocumentos" -e pdf,docx -o "D:\reportes"
```

#### Argumentos disponibles

| Argumento | Descripción | Obligatorio |
|-----------|-------------|-------------|
| `<PATH>` | Ruta de la carpeta a escanear | ✅ Sí |
| `-e`, `--ext` | Extensiones a filtrar (separadas por coma o repetidas) | ❌ No |
| `-o`, `--output` | Directorio donde guardar los reportes (por defecto: misma ruta de entrada) | ❌ No |
| `-h`, `--help` | Muestra la ayuda | — |
| `-V`, `--version` | Muestra la versión | — |

---

## 📁 Archivos generados

Al finalizar, se generan **tres archivos** en la carpeta de salida:

### `report.csv`

Tabla plana con un registro por archivo, ideal para abrir en Excel o procesar con Pandas:

| Campo | Descripción |
|-------|-------------|
| `name` | Nombre del archivo (sin extensión) |
| `extension` | Extensión (sin punto) |
| `size_mb` | Tamaño en MB con 3 decimales |
| `created` | Fecha de creación (DD/MM/AAAA) |
| `modified` | Fecha de última modificación (DD/MM/AAAA) |
| `directory` | Ruta del directorio padre |
| `full_path` | Ruta completa del archivo |

> 💡 **Compatible con Excel:** UTF-8 con BOM y separador `;` para evitar problemas de codificación y alineación de columnas.

### `report.md`

Árbol jerárquico estilo `tree` listo para incluir en documentación:

````markdown
# 📋 Estructura de Directorios

```
D:\MiCarpeta
├── carpeta1
│   └── archivo1.mp4
└── carpeta2
    ├── archivo2.txt
    └── subcarpeta
        └── archivo3.jpg
```
````

### `report.json`

Estructura jerárquica completa, útil para automatización e integraciones:

```json
{
  "type": "directory",
  "name": "MiCarpeta",
  "path": "D:\\MiCarpeta",
  "children": [
    {
      "type": "file",
      "name": "documento.pdf",
      "size_mb": 1.234,
      "extension": "pdf",
      "created": "2024-03-15T14:23:01Z",
      "modified": "2024-03-15T14:23:01Z"
    }
  ]
}
```

---

## 🏗️ Arquitectura del proyecto

```
file-reporter/
├── Cargo.toml
└── src/
    ├── main.rs          # Punto de entrada: decide CLI vs GUI
    ├── cli.rs           # Definición de argumentos con clap
    ├── gui.rs           # Interfaz gráfica con egui/eframe
    ├── runner.rs        # Lógica común compartida CLI ↔ GUI
    ├── scanner.rs       # Escaneo paralelo con rayon
    ├── models.rs        # Estructuras de datos (Node, FileRecord)
    └── reporters/
        ├── mod.rs
        ├── csv.rs       # Generación CSV (BOM + ;)
        ├── markdown.rs  # Árbol estilo `tree`
        └── json.rs      # JSON pretty-printed
```

### Diseño modular

- **`runner.rs`** centraliza la lógica de ejecución, permitiendo que CLI y GUI compartan exactamente el mismo flujo.
- **`scanner.rs`** usa `rayon` para procesar directorios en paralelo, con manejo robusto de errores por archivo (las carpetas sin permisos se omiten en lugar de detener todo el escaneo).
- **`gui.rs`** ejecuta el escaneo en un hilo separado con comunicación vía `mpsc::channel` y reporte de progreso compartido con `Arc<AtomicUsize>`, manteniendo la UI 100% responsive.

---

## 🧰 Tecnologías utilizadas

| Crate | Uso |
|-------|-----|
| [`clap`](https://crates.io/crates/clap) | Parseo de argumentos CLI |
| [`eframe`](https://crates.io/crates/eframe) / [`egui`](https://crates.io/crates/egui) | Interfaz gráfica nativa |
| [`rfd`](https://crates.io/crates/rfd) | Diálogo nativo de selección de carpetas |
| [`rayon`](https://crates.io/crates/rayon) | Paralelismo data-parallel |
| [`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json) | Serialización JSON |
| [`csv`](https://crates.io/crates/csv) | Generación CSV |
| [`chrono`](https://crates.io/crates/chrono) | Fechas y formatos |
| [`anyhow`](https://crates.io/crates/anyhow) | Manejo ergonómico de errores |

---

## 🐛 Manejo de errores

- **Rutas inválidas o inexistentes:** mensaje de error claro, sin crash.
- **Permisos denegados:** las carpetas inaccesibles se omiten silenciosamente y el escaneo continúa.
- **Caracteres especiales en rutas (UTF-8):** soportados correctamente; los acentos y emojis se preservan en los tres reportes.
- **GUI:** los errores se muestran en un panel rojo dentro de la ventana, sin cerrar la aplicación. Cualquier panic interno del worker se captura con `catch_unwind`.

---

## 🚧 Posibles mejoras futuras

- [ ] Persistir las últimas rutas usadas entre ejecuciones.
- [ ] Lista configurable de carpetas a excluir (`.git`, `node_modules`, etc.).
- [ ] Toggle modo claro/oscuro.
- [ ] Tamaño total de archivos en el resumen.
- [ ] Vista previa del árbol antes de exportar.
- [ ] Empaquetar como instalador `.msi` con `cargo-wix`.
- [ ] Tests unitarios con `tempfile`.

---

## 📝 Licencia

Este proyecto se distribuye bajo la licencia **MIT**. Consulta el archivo [`LICENSE`](LICENSE) para más detalles.

---

## 👤 Autor

Desarrollado como proyecto de aprendizaje en Rust.  
*Si te resulta útil, considera dejar una ⭐ en el repositorio.*

---

## 🙋 Contribuciones

Las contribuciones son bienvenidas. Para cambios mayores:

1. Abre un issue describiendo la propuesta.
2. Haz un fork y crea una rama para tu feature.
3. Envía un pull request con descripción clara de los cambios.