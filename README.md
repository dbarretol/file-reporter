# Compilar en modo release (recomendado para grandes volúmenes)
cargo build --release

# Escanear todo
./target/release/file-reporter /ruta/a/escanear

# Filtrar por extensiones
./target/release/file-reporter /ruta -e txt -e jpg -e pdf

# Especificar directorio de salida
./target/release/file-reporter /ruta -e mp4 -o /ruta/reportes

# También acepta lista separada por comas
./target/release/file-reporter /ruta --ext txt,jpg,png

🔧 Posibles mejoras futuras
Barra de progreso con indicatif para escaneos largos.
Logging con tracing o env_logger en lugar de println!.
Configuración por archivo (config.toml) con figment o config.
Exclusiones (ej: ignorar .git, node_modules).
Tests unitarios en scanner.rs con tempfile.
Streaming del CSV para no cargar todos los registros en memoria si son millones.