# 📋 File Reporter

[![CI](https://github.com/dbarretol/file-reporter/actions/workflows/ci.yml/badge.svg)](https://github.com/dbarretol/file-reporter/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)

**Escanea una carpeta y obtén, en segundos, un inventario completo de su contenido en CSV, Markdown y JSON.**

File Reporter es una herramienta escrita en Rust con **interfaz gráfica** (para uso diario) y **línea de comandos** (para scripts y automatización). Recorre el directorio en paralelo y genera tres reportes con la estructura de carpetas y los metadatos de cada archivo.

```text
$ file-reporter C:\Demo\Proyecto

✅ Reportes generados correctamente:
   📄 CSV:      C:\Demo\Proyecto\report.csv
   📄 Markdown: C:\Demo\Proyecto\report.md
   📄 JSON:     C:\Demo\Proyecto\report.json

📊 Archivos procesados: 4
⏱️  Tiempo: 1.98ms
```

```text
C:\Demo\Proyecto
├── docs
│   └── manual.pdf
├── src
│   ├── utils
│   │   └── helpers.rs
│   └── main.rs
└── README.md
```

---

## Contenido

- [¿Para qué sirve?](#-para-qué-sirve)
- [Características](#-características)
- [Instalación](#-instalación)
- [Uso](#-uso)
- [Reportes generados](#-reportes-generados)
- [Cómo funciona](#-cómo-funciona)
- [Desarrollo](#-desarrollo)
- [Hoja de ruta](#-hoja-de-ruta)
- [Contribuciones](#-contribuciones)
- [Licencia](#-licencia)

---

## 💡 ¿Para qué sirve?

- **Auditar un disco o carpeta compartida:** qué hay, cuánto ocupa y cuándo se modificó, en una hoja de Excel.
- **Documentar la estructura de un proyecto:** pega el árbol de `report.md` en tu documentación.
- **Inventariar tipos concretos de archivo:** por ejemplo, todos los `pdf` y `docx` de una unidad.
- **Alimentar otros procesos:** el JSON jerárquico se integra fácilmente en scripts o pipelines.

---

## ✨ Características

**Reportes**
- 📊 Tres formatos a la vez: **CSV** (listo para Excel), **Markdown** (árbol estilo `tree`) y **JSON** (jerárquico).
- 📅 Metadatos por archivo: nombre, extensión, tamaño (MB y bytes exactos), fechas de creación y modificación, y ruta.
- 🎯 **Filtro por extensiones** (`pdf, docx, jpg…`).
- 📂 **Modo solo carpetas:** genera únicamente la jerarquía de directorios, sin archivos.

**Rendimiento y robustez**
- 🚀 Escaneo **paralelo** con [`rayon`](https://crates.io/crates/rayon), en una sola pasada por el disco.
- 🛡️ Sigue funcionando ante carpetas sin permisos y avisa cuántos elementos se omitieron.
- 🔁 Detecta symlinks y junctions cíclicos para no quedarse colgado.
- 🔒 CSV protegido contra *formula injection*.

**Interfaz gráfica**
- 🖱️ Arrastra y suelta una carpeta sobre la ventana, o usa "Examinar…".
- 📈 Barra de progreso con tiempo estimado y botón para **cancelar** en cualquier momento.
- ⚠️ Pide confirmación antes de sobrescribir reportes anteriores.
- ♻️ Recuerda rutas y opciones entre sesiones.
- 🎨 Tema claro u oscuro según el sistema operativo.

---

## 📦 Instalación

### Requisitos

- [Rust](https://rustup.rs/) (toolchain estable).
- **Solo en Linux:** dependencias del sistema para la GUI y los diálogos nativos:
  ```bash
  sudo apt install libgtk-3-dev libxkbcommon-dev
  ```

### Compilar desde el código fuente

```bash
git clone https://github.com/dbarretol/file-reporter.git
cd file-reporter
cargo build --release
```

El ejecutable queda en `target/release/file-reporter` (`file-reporter.exe` en Windows).

> Probado principalmente en **Windows**; también compila en Linux y macOS.

---

## 🚀 Uso

El mismo ejecutable funciona en dos modos:

| Cómo lo ejecutas | Modo |
|---|---|
| Sin argumentos (o doble clic en el `.exe`) | 🖥️ Interfaz gráfica |
| Con una ruta como argumento | ⌨️ Línea de comandos |

### 🖥️ Interfaz gráfica

```bash
./target/release/file-reporter
```

1. Elige la **carpeta a escanear**: escribe la ruta, pulsa "Examinar…" o arrástrala a la ventana.
2. *(Opcional)* Elige una **carpeta de salida**. Por defecto, la misma carpeta escaneada.
3. *(Opcional)* Escribe las **extensiones** a incluir o usa una de las sugerencias.
4. *(Opcional)* Marca **Solo carpeta (omitir archivos)** para obtener solo la jerarquía de directorios.
5. Pulsa **▶ Generar reportes** (o Enter).

### ⌨️ Línea de comandos

```bash
# Todo el contenido de una carpeta
file-reporter /ruta/a/escanear

# Solo ciertas extensiones (separadas por coma o con -e repetido)
file-reporter /ruta --ext pdf,docx
file-reporter /ruta -e jpg -e png

# Guardar los reportes en otra carpeta
file-reporter /ruta -o /ruta/reportes

# Solo la estructura de carpetas
file-reporter /ruta -d
```

En Windows (PowerShell):

```powershell
.\file-reporter.exe "D:\MisDocumentos" -e pdf,docx -o "D:\reportes"
```

| Opción | Descripción |
|---|---|
| `<PATH>` | Carpeta a escanear **(obligatorio)** |
| `-e`, `--ext` | Extensiones a incluir. Sin esta opción se incluyen todas |
| `-o`, `--output` | Carpeta de salida (por defecto: la carpeta escaneada) |
| `-d`, `--dirs-only` | Solo estructura de carpetas. No se puede combinar con `-e` |
| `-h`, `--help` / `-V`, `--version` | Ayuda / versión |

> **Sobre las extensiones:** se usa lo que va tras el **último** punto del nombre. `foo.tar.gz` tiene extensión `gz`, no `tar.gz`. Los archivos sin punto (`LICENSE`, `Makefile`) no tienen extensión y quedan fuera de cualquier filtro.

---

## 📁 Reportes generados

Cada ejecución crea tres archivos en la carpeta de salida:

| Archivo | Contenido | Ideal para |
|---|---|---|
| `report.csv` | Una fila por archivo con todos sus metadatos | Excel, Pandas, hojas de cálculo |
| `report.md` | Árbol de carpetas y archivos estilo `tree` | Documentación, READMEs |
| `report.json` | Árbol completo con metadatos | Scripts e integraciones |

En **modo solo carpetas**, `report.md` y `report.json` contienen únicamente directorios y `report.csv` queda solo con la cabecera.

<details>
<summary><b>📊 report.csv</b>: columnas</summary>

<br>

UTF-8 con BOM y separador `;`, para que Excel lo abra correctamente sin configurar nada.

| Columna | Descripción |
|---|---|
| `name` | Nombre del archivo, sin extensión |
| `extension` | Extensión, sin punto |
| `size_mb` | Tamaño en MB (hasta 3 decimales) |
| `size_bytes` | Tamaño exacto en bytes |
| `created` / `modified` | Fechas en `DD/MM/AAAA` (formato que Excel reconoce directamente) |
| `created_iso` / `modified_iso` | Fecha y hora en ISO-8601, ordenables como texto |
| `directory` | Carpeta que contiene el archivo |
| `full_path` | Ruta completa |

```text
name;extension;size_mb;size_bytes;created;modified;created_iso;modified_iso;directory;full_path
manual;pdf;1.27;1331200;23/10/2026;23/10/2026;2026-10-23T04:42:45.034591400+00:00;...;C:\Demo\Proyecto\docs;C:\Demo\Proyecto\docs\manual.pdf
```

Si un nombre o ruta empieza por `=`, `+`, `-`, `@`, tabulador o retorno de carro, se le antepone un apóstrofo para que Excel no lo ejecute como fórmula.

</details>

<details>
<summary><b>🌳 report.md</b>: árbol de directorios</summary>

<br>

````markdown
# 📋 Estructura de Directorios

```
C:\Demo\Proyecto
├── docs
│   └── manual.pdf
├── src
│   ├── utils
│   │   └── helpers.rs
│   └── main.rs
└── README.md
```
````

</details>

<details>
<summary><b>🧩 report.json</b>: estructura jerárquica</summary>

<br>

```json
{
  "type": "directory",
  "name": "Proyecto",
  "path": "C:\\Demo\\Proyecto",
  "children": [
    {
      "type": "directory",
      "name": "docs",
      "path": "C:\\Demo\\Proyecto\\docs",
      "children": [
        {
          "type": "file",
          "name": "manual.pdf",
          "path": "C:\\Demo\\Proyecto\\docs\\manual.pdf",
          "size_mb": 1.26953125,
          "size_bytes": 1331200,
          "extension": "pdf",
          "created": "2026-10-23T04:42:45.034591400Z",
          "modified": "2026-10-23T04:42:45.034591400Z"
        }
      ]
    }
  ]
}
```

</details>

---

## 🔧 Cómo funciona

CLI y GUI comparten exactamente la misma lógica; solo cambia la forma de recibir la configuración y de mostrar el progreso.

```text
  CLI (clap) ──┐
               ├──► runner ──► scanner (rayon, en paralelo) ──► árbol en memoria ──► reporters ──► CSV / MD / JSON
  GUI (egui) ──┘       ▲
                       └── progreso, cancelación y elementos omitidos (atómicos compartidos)
```

```text
src/
├── main.rs          # Punto de entrada: decide entre CLI y GUI
├── cli.rs           # Argumentos de línea de comandos (clap)
├── gui.rs           # Interfaz gráfica (egui/eframe)
├── runner.rs        # Flujo común: escanear → escribir reportes
├── scanner.rs       # Recorrido paralelo del directorio
├── models.rs        # Node (árbol) y FileRecord (fila del CSV)
└── reporters/
    ├── csv.rs       # CSV para Excel (BOM + ;)
    ├── markdown.rs  # Árbol estilo tree
    └── json.rs      # JSON con formato legible
```

<details>
<summary><b>Detalles de implementación</b></summary>

<br>

- **Una sola pasada por el disco:** no se cuentan los archivos antes de escanearlos. El progreso se reporta en vivo mientras se recorre el árbol.
- **Symlinks y junctions:** se usa `symlink_metadata` para no seguir enlaces y evitar ciclos. La carpeta raíz elegida por el usuario sí se sigue aunque sea un enlace (por ejemplo, carpetas de OneDrive).
- **Errores por elemento:** una carpeta sin permisos se omite y se contabiliza; no detiene el escaneo.
- **GUI siempre responsiva:** el escaneo corre en un hilo aparte con pila ampliada y `catch_unwind`, de modo que un panic interno se muestra como error y no cierra la aplicación. La cancelación se distingue de un error real mediante una bandera compartida.
- **Rutas Unicode:** acentos y emojis se conservan en los tres reportes, y la exclusión de los propios reportes compara rutas sin distinguir mayúsculas, también fuera de ASCII.

</details>

### Tecnologías

[`clap`](https://crates.io/crates/clap) (CLI) ·
[`egui`/`eframe`](https://github.com/emilk/egui) (GUI) ·
[`rfd`](https://crates.io/crates/rfd) (diálogos nativos) ·
[`rayon`](https://crates.io/crates/rayon) (paralelismo) ·
[`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json) ·
[`csv`](https://crates.io/crates/csv) ·
[`chrono`](https://crates.io/crates/chrono) ·
[`anyhow`](https://crates.io/crates/anyhow)

> `egui`/`eframe` están fijados en la versión `0.30` a propósito: las siguientes traen cambios de API que requieren una migración aparte (ver el comentario en `Cargo.toml`).

---

## 🧪 Desarrollo

```bash
cargo test                          # tests
cargo fmt --check                   # formato
cargo clippy --all-targets -- -D warnings   # lint
```

Los tests trabajan sobre un sistema de archivos real (carpetas temporales con [`tempfile`](https://crates.io/crates/tempfile)) y cubren escaneo, filtros, modo solo carpetas, cancelación, los tres formatos de salida y la protección del CSV. En Windows también crean *junctions* para probar el manejo de enlaces sin necesidad de permisos de administrador.

Cada push a `master` y cada pull request pasan por [GitHub Actions](.github/workflows/ci.yml) en Windows y Ubuntu.

---

## 🧭 Hoja de ruta

- [ ] Lista configurable de carpetas a excluir (`.git`, `node_modules`, …).
- [ ] Tamaño total en el resumen final.
- [ ] Vista previa del árbol antes de exportar.
- [ ] Selector manual de tema claro/oscuro.
- [ ] Binarios precompilados en *Releases* e instalador `.msi`.
- [ ] Migración a egui/eframe 0.35+.

---

## 🤝 Contribuciones

Las contribuciones son bienvenidas. Para cambios grandes, abre primero un *issue* para comentar la propuesta. Después, haz un fork, crea una rama y envía un *pull request* describiendo los cambios.

---

## 📝 Licencia

Distribuido bajo la licencia **MIT**. Consulta [`LICENSE`](LICENSE).

Desarrollado por [@dbarretol](https://github.com/dbarretol) como proyecto de aprendizaje en Rust. Si te resulta útil, deja una ⭐.
