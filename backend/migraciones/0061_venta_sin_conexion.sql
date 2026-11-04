-- Módulo VENTA_SIN_CONEXION: vender sin internet y subir todo al volver la
-- conexión (handlers/sin_conexion.rs).
--
-- Solo ADITIVO: una tabla nueva. No toca ventas, cajas, gastos, comprobantes
-- ni ningún trigger.
--
--   operaciones_sin_conexion : cada operación hecha en el equipo sin internet
--     (una venta, una apertura de caja o un gasto), con el código único que
--     le puso el equipo. Sirve para que la misma operación nunca entre dos
--     veces, aunque el equipo la vuelva a mandar (se cortó la respuesta, se
--     cargó un respaldo, etc.).
--       uuid        : código que puso el equipo (único).
--       tipo        : 'VENTA', 'ABRIR_CAJA' o 'GASTO'.
--       estado      : 'EN_PROCESO' (se está guardando), 'VENTA_HECHA' (la
--                     venta ya entró; falta o falló su comprobante) o
--                     'HECHA' (terminada).
--       venta_id, gasto_id, caja_id : lo que se creó.
--       fecha_local : cuándo se hizo en el equipo ("AAAA-MM-DD HH:MM:SS").
--       resultado   : lo que se le respondió al equipo (JSON), para repetirlo.
--       recibida    : cuándo llegó al servidor.
--
-- Una venta del punto de venta con el módulo encendido también lleva su
-- código: si se corta la respuesta y el equipo la guarda para subirla
-- después, aquí se reconoce que ya había entrado.

CREATE TABLE IF NOT EXISTS operaciones_sin_conexion (
  uuid TEXT PRIMARY KEY,
  tipo TEXT NOT NULL,
  estado TEXT NOT NULL DEFAULT 'EN_PROCESO',
  venta_id INTEGER,
  gasto_id INTEGER,
  caja_id INTEGER,
  usuario_id INTEGER,
  fecha_local TEXT,
  resultado TEXT,
  recibida TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_operaciones_sin_conexion_venta ON operaciones_sin_conexion (venta_id);
