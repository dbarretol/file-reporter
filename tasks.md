# Tasks — Auditoría de código (2026-07-18)

Resultado de la auditoría de bugs, UI/UX, usabilidad y mejoras.
Orden sugerido de trabajo: P0 → P1 → P2 → P3.

---

## P0 — Bugs críticos (corrección funcional)

- [ ] **1. Recursión infinita con symlinks / junctions** — `src/scanner.rs:18`, `src/runner.rs:117`
  `build_tree` y `count_files` usan `fs::metadata` / `p.is_dir()`, que **siguen** enlaces simbólicos.
  En Windows existen junctions cíclicas por defecto (p. ej. `C:\Users\<u>\AppData\Local\Application Data`),
  y en Unix un symlink a un ancestro produce recursión infinita → cuelgue o stack overflow.
  **Fix:** usar `fs::symlink_metadata` y omitir (o registrar sin descender) entradas cuyo
  `file_type().is_symlink()` sea true.
  **Criterio de aceptación:** escanear `C:\Users\<usuario>` completo termina sin colgarse.

- [ ] **2. El progreso real es ficticio: la barra se queda en 0 % durante el escaneo** — `src/runner.rs:79-84`, `src/scanner.rs:125-127`
  El contador `done` solo se incrementa en `flatten_to_records`, que es una operación
  **en memoria** posterior al escaneo. Todo el trabajo pesado (I/O de `build_tree`) ocurre
  con la barra en 0 %, y luego salta a 100 % en milisegundos. El ETA calculado sobre esa
  fracción no significa nada.
  **Fix:** incrementar el contador dentro de `build_tree` cada vez que se procesa un archivo
  (pasar el `AtomicUsize` al scanner), y quitar el incremento de `flatten_to_records`.
  **Criterio:** en una carpeta con >50k archivos la barra avanza de forma continua durante el escaneo.

- [ ] **3. Al lanzar la GUI desde una terminal, se oculta la ventana de la terminal del usuario** — `src/main.rs:61-74`
  `ShowWindow(GetConsoleWindow(), SW_HIDE)` oculta la consola **adjunta**, que al ejecutar
  `file-reporter.exe` sin argumentos desde PowerShell/cmd es la terminal del propio usuario:
  su ventana desaparece y no vuelve.
  **Fix:** antes de ocultar, comprobar con `GetConsoleProcessList` que el proceso es el único
  dueño de la consola (caso doble-clic); si hay más procesos (terminal interactiva), no ocultarla
  (o usar `FreeConsole`).
  **Criterio:** doble clic → sin consola visible; ejecutar desde PowerShell → la terminal sigue visible.

- [ ] **4. Los reportes de ejecuciones anteriores se incluyen en el nuevo escaneo** — `src/runner.rs:64-102`
  Por defecto `output == input`, así que `report.csv/md/json` de la corrida anterior aparecen
  como archivos escaneados en la siguiente (y el conteo del progreso también los incluye).
  Además, `create_dir_all(output)` se ejecuta **antes** de escanear, por lo que un output
  dentro del input crea una carpeta que se incluye en el árbol.
  **Fix:** excluir del escaneo los tres paths de salida exactos (o toda la carpeta de salida si
  está dentro del input); crear el directorio de salida después del escaneo, o excluirlo.
  **Criterio:** dos corridas seguidas sobre la misma carpeta arrojan el mismo `files_count`.

- [ ] **5. "Tiempo total" sigue creciendo después de terminar** — `src/gui.rs:485-490`
  Al terminar, `compute_eta` muestra `started_at.elapsed()`, que se recalcula en cada repaint:
  si el usuario interactúa 5 minutos después, ve "Tiempo total: 5m…" en vez de la duración real.
  **Fix:** guardar `finished_at` (o la duración) al recibir `WorkerMsg::Done/Failed` y mostrar eso.
  **Criterio:** el tiempo total mostrado queda congelado tras completar.

---

## P1 — Bugs de UX / usabilidad (alto impacto)

- [ ] **6. La sincronización input→output captura texto a medio escribir** — `src/gui.rs:216-218`
  El auto-relleno se dispara en cuanto `input_path` no está vacío. Si el usuario **teclea** la ruta
  en vez de usar "Examinar…", al primer carácter (`C`) el output queda fijado en `"C"` y ya no se
  actualiza. Además impide dejar el output vacío (el hint "Por defecto: misma que la entrada"
  nunca se llega a ver porque el campo se rellena solo).
  **Fix:** no auto-rellenar el campo; dejar el output vacío con su hint y resolver el default en
  `start_run` (que ya lo hace en `src/gui.rs:436-440`). Si se quiere mostrar el valor efectivo,
  hacerlo como texto informativo, no escribiéndolo en el campo.
  **Criterio:** teclear la ruta de entrada manualmente no contamina el campo de salida.

- [ ] **7. No hay forma de cancelar un escaneo en curso** — `src/gui.rs`, `src/runner.rs`
  Con una carpeta enorme o una unidad de red lenta, la GUI queda en "Procesando…" sin salida
  posible (los campos y botones quedan deshabilitados). El campo `Progress.finished` existe
  pero **nunca se lee** (código muerto).
  **Fix:** añadir un `AtomicBool cancel` a `Progress`, botón "⏹ Cancelar" visible durante
  `Running`, y chequear la bandera en `build_tree`/`count_files` (retornando error "Cancelado").
  Aprovechar para eliminar o usar `finished`.
  **Criterio:** cancelar durante un escaneo grande devuelve la GUI a estado utilizable en <1 s.

- [ ] **8. Rutas pegadas con comillas fallan** — `src/gui.rs:435-440`
  "Copiar como ruta de acceso" de Windows pega `"C:\..."` con comillas; el run falla con
  "La ruta no existe" sin pista del motivo.
  **Fix:** aplicar `trim_matches('"')` (además del `trim()` actual) a input y output.
  **Criterio:** pegar una ruta con comillas funciona.

- [ ] **9. Sobrescritura silenciosa de reportes existentes** — `src/runner.rs:96-98`
  Los nombres `report.csv/md/json` son fijos: cada corrida pisa la anterior sin avisar.
  **Fix (elegir una):** (a) confirmar sobrescritura en la GUI, o (b) nombres con timestamp
  (`report_2026-07-18_1530.csv`) opcionales. La (b) además elimina parte del problema #4.
  **Criterio:** el usuario no pierde reportes previos sin enterarse.

- [ ] **10. Errores de lectura ignorados sin rastro** — `src/scanner.rs:29-32,37`
  Carpetas sin permiso se tratan como vacías y archivos con error se descartan con `.ok()`.
  El usuario ve "Completado" creyendo que el reporte es completo cuando puede faltar media unidad.
  **Fix:** contar entradas omitidas (`AtomicUsize skipped` en `Progress`/`RunSummary`) y mostrar
  en resultados: "⚠ N elementos no accesibles fueron omitidos".
  **Criterio:** escanear una carpeta con subcarpetas sin permiso lo informa en el resultado.

---

## P2 — Mejoras de UI/UX (pulido)

- [ ] **11. Doble recorrido de disco: `count_files` duplica todo el I/O** — `src/runner.rs:71-76,117-134`
  El pre-conteo recorre el árbol completo y luego `build_tree` lo vuelve a recorrer: el tiempo
  total casi se duplica solo para tener un ETA. Con la tarea #2 hecha, evaluar contar en un
  hilo/estimación incremental, o aceptar barra indeterminada + contador "N archivos encontrados"
  durante el conteo.
  **Criterio:** el escaneo no lee el disco dos veces (o el costo extra queda justificado y documentado).

- [ ] **12. Drag & drop de carpetas** — `src/gui.rs`
  egui expone `ctx.input(|i| i.raw.dropped_files)`. Arrastrar una carpeta a la ventana debería
  rellenar "Carpeta a escanear". Es el gesto más natural para esta app.

- [ ] **13. Recordar últimas rutas y extensiones** — `src/gui.rs:22-27`
  eframe trae persistencia (`cc.storage`, feature `persistence`). Restaurar input/output/extensiones
  de la sesión anterior ahorra el paso más repetitivo.

- [ ] **14. "Abrir carpeta de salida" debería seleccionar el archivo, y los errores no se reportan** — `src/gui.rs:388-397,580-586`
  Con `explorer /select,<ruta>` se abre la carpeta con el CSV resaltado. Además ambos botones
  ignoran errores (`let _ =`): si explorer falla, el clic no hace nada visible.
  **Fix:** usar `/select,` para "Abrir carpeta", y mostrar el error en la status bar si el spawn falla.

- [ ] **15. Tamaños "0.000 MB" para archivos pequeños** — `src/scanner.rs:112`, reportes
  Todo archivo <524 bytes reporta 0.000. Mostrar tamaño adaptativo (B/KB/MB/GB) en columnas
  legibles, o añadir columna `size_bytes` exacta al CSV/JSON para poder ordenar y sumar.

- [ ] **16. Enter para ejecutar** — `src/gui.rs:196-266`
  Con el foco en cualquier campo, Enter debería disparar "Generar reportes" si `can_run`.

- [ ] **17. Estado "Contando archivos…" mezclado con la fase real** — `src/gui.rs:341-345`
  El texto se decide solo por `total == 0`; con #2 y #11 resueltos, modelar las fases
  explícitamente (Contando → Escaneando → Generando reportes) y mostrarlas en la barra de estado.

- [ ] **18. Contraste/estilo en tema claro** — `src/gui.rs:80-83`
  `MUTED` (150,150,150) sobre fondo claro roza el mínimo de contraste, y `ACCENT/SUCCESS/ERROR`
  están calibrados para tema oscuro. Verificar ambos temas (egui sigue el del sistema) y ajustar
  colores por tema (`ui.visuals().dark_mode`).

---

## P3 — Calidad de código y deuda técnica

- [ ] **19. Cero tests** — todo el crate
  Nada tiene cobertura. Prioridad de tests unitarios (con `tempfile` como dev-dependency):
  - `scanner::build_tree`: filtrado por extensión, dirs vacíos tras filtrar, orden dirs-primero.
  - `scanner::flatten_to_records`: conteo y campos del `FileRecord`.
  - `runner::run`: input inexistente / no-directorio, corrida feliz genera los 3 archivos.
  - `truncate_path_display` y `fmt_seconds` (`src/gui.rs:555-578`): bordes UTF-8 y 0 s/59 s/1 h.
  - `Cli::normalized_extensions`: puntos y mayúsculas.

- [ ] **20. `file-reporter.exe` suelto en la raíz del repo** — raíz
  Binario sin trackear junto al código. Borrarlo y añadir `*.exe` al `.gitignore` (el build
  vive en `target/`).

- [ ] **21. Extensiones compuestas y archivos sin extensión no documentados** — `src/scanner.rs:62-66`, `README.md`
  `path.extension()` da `gz` para `foo.tar.gz`, y los archivos sin extensión son imposibles de
  incluir cuando hay filtro activo. Documentar la semántica en README y en el hint de la GUI;
  opcionalmente aceptar un token especial (p. ej. `(sin ext)`) en el filtro.

- [ ] **22. Fechas del CSV no ordenables y sin hora** — `src/scanner.rs:113-119`
  `DD/MM/YYYY` no ordena como texto y descarta la hora. Mantenerlo para Excel es válido, pero
  el JSON ya lleva fechas completas; considerar columna adicional ISO-8601 en CSV o dejarlo
  documentado como decisión.

- [ ] **23. Recursión profunda puede agotar la pila** — `src/scanner.rs:18-77`, `src/runner.rs:117-134`
  `build_tree` recursa dentro de hilos rayon (pila por defecto ~2 MB). Árboles muy profundos
  (paths largos de node_modules, unidades con anidamiento patológico) pueden reventar la pila.
  Bajo riesgo; mitigar con recorrido iterativo (stack explícito) o documentar el límite.

- [ ] **24. Ajustes de release** — `Cargo.toml:20-22`
  Añadir `strip = true` (reduce varios MB del binario). **No** usar `panic = "abort"`: rompería
  el `catch_unwind` del worker de la GUI (`src/gui.rs:456`).

- [ ] **25. Dependencias desactualizadas** — `Cargo.toml`
  `eframe/egui 0.28` y `rfd 0.14` tienen varias versiones más recientes con fixes de plataforma.
  Migrar (la API de `Rounding`→`CornerRadius` y `Margin` cambia en 0.29+); hacerlo después de
  las tareas P0/P1 para no mezclar refactor con fixes.

---

## Notas de la auditoría

Cosas revisadas que **están bien** y no requieren acción:
- El worker captura panics (`catch_unwind`) y rayon propaga panics de `par_iter` al caller, así que la GUI no se cae.
- `Progress::fraction` está protegido contra NaN/fuera de rango.
- El CSV escribe BOM UTF-8 y usa `;` — correcto para Excel es-ES.
- `truncate_path_display` es seguro con UTF-8 (opera sobre chars, no bytes).
- El botón "Generar" se deshabilita durante `Running`, así que no hay corridas concurrentes.
