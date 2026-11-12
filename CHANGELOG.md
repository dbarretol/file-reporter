# Registro de cambios

Los cambios notables de cada versión. El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto usa [versionado semántico](https://semver.org/lang/es/).

## [0.4.0] - 2026-11-11

### Añadido

- **Progreso en la línea de comandos:** mientras escanea, la CLI muestra cuántos archivos (o carpetas) lleva encontrados y el tiempo transcurrido, y después que está escribiendo los reportes. No aparece si la salida se redirige ([#7](https://github.com/dbarretol/file-reporter/issues/7)).
- **`-v/--verbose` en la CLI:** lista en stderr qué se omitió y por qué (carpetas y archivos sin acceso, con el error del sistema; enlaces no seguidos; carpetas excluidas por nombre). `-vv` añade el detalle del proceso. Sin `-v`, el aviso de elementos omitidos sugiere usarlo ([#8](https://github.com/dbarretol/file-reporter/issues/8)).

## [0.3.0] - 2026-11-11

### Añadido

- **Ejecutables precompilados** para Windows y Linux en cada *Release* de GitHub: ya no hace falta instalar Rust para usar la herramienta ([#15](https://github.com/dbarretol/file-reporter/issues/15)).
- **Omitir carpetas por nombre** en cualquier nivel, sin llegar a leerlas: `-x/--exclude` y `--exclude-common` (`.git`, `node_modules`, `target`, `__pycache__`, `.venv`, `dist`, `build`) en la CLI, y campo "Carpetas a omitir" en la GUI, que parte con esa lista ([#4](https://github.com/dbarretol/file-reporter/issues/4)).
- **Resumen por tipo de archivo:** cantidad, tamaño y porcentaje de cada extensión en la consola, en la GUI y como tabla en `report.md` ([#17](https://github.com/dbarretol/file-reporter/issues/17)).
- **Elegir los formatos** a generar: `-f csv,md,json` en la CLI y casillas en la GUI.
- **GUI:**
  - El resultado se abre en una ventana propia, con los reportes (abrir o copiar su ruta), la carpeta de salida y los tipos de archivo; el botón **Ver resultado** la vuelve a abrir.
  - Validación de las rutas mientras se escriben e indicación de dónde se guardarán los reportes.
  - Contador en vivo y tiempo transcurrido; Esc cancela el escaneo.
  - Tamaño total en el resumen ([#5](https://github.com/dbarretol/file-reporter/issues/5)).
  - Parpadeo en la barra de tareas al terminar si la ventana está en segundo plano ([#14](https://github.com/dbarretol/file-reporter/issues/14)).
  - Selector de tema **Sistema · Claro · Oscuro**, recordado entre sesiones ([#11](https://github.com/dbarretol/file-reporter/issues/11)).
  - Las sugerencias de extensiones se suman a lo escrito y, pulsadas otra vez, se quitan.
- **Icono propio** en la ventana, la barra de tareas y el `.exe`, que además muestra nombre y versión en sus propiedades ([#6](https://github.com/dbarretol/file-reporter/issues/6)).

### Cambiado

- **Escaneo más rápido y con menos memoria** (−32 % de tiempo y −29 % de memoria en la medición de referencia): los metadatos salen de la propia lectura del directorio y el CSV se escribe directamente desde el árbol ([#10](https://github.com/dbarretol/file-reporter/issues/10)).
- La GUI usa egui/eframe 0.37; la versión mínima de Rust pasa a **1.95**.
- La barra de progreso de la GUI, que anunciaba un tiempo estimado que nunca se calculaba, se sustituye por el contador y el tiempo transcurrido.

### Corregido

- CLI: las extensiones escritas con espacios tras la coma (`-e "pdf, docx"`) no se filtraban ([#19](https://github.com/dbarretol/file-reporter/issues/19)).
- GUI: los mensajes de error muestran la causa completa, no solo el contexto ([#20](https://github.com/dbarretol/file-reporter/issues/20)).
- GUI: al arrancar con un tema distinto al del sistema, los botones de la barra de título de Windows quedaban casi invisibles.

### Desarrollo

- Perfil `fast` (`cargo run --profile fast`): optimizado sin LTO, recompila en segundos.
- La CI de Windows pasa de ~3 a ~1 minuto.
- La GUI se divide en módulos (`src/gui/`).

## [0.2.0] - 2026-10-22

Primera versión pública.

- CLI y GUI en el mismo ejecutable.
- Reportes en CSV (listo para Excel), Markdown (árbol estilo `tree`) y JSON (jerárquico), con los metadatos de cada archivo.
- Filtro por extensiones y modo solo carpetas.
- Escaneo paralelo en una sola pasada; tolera carpetas sin permisos y enlaces cíclicos.
- CSV protegido contra *formula injection*.

[0.4.0]: https://github.com/dbarretol/file-reporter/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dbarretol/file-reporter/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dbarretol/file-reporter/releases/tag/v0.2.0
