-- Registro de lotes descartados (merma), para el reporte de productos
-- vencidos y la auditoría del inventario.
--
-- Solo ADITIVO: una tabla nueva. No toca productos, lotes ni ventas.
--
--   mermas_lote : una fila por cada lote descartado desde Stock y lotes.
--     Guarda una FOTO de cómo estaba el lote y el producto en ese momento
--     (nombre, unidad, número de lote, vencimiento, cantidad y costo), así
--     el reporte sigue diciendo lo mismo aunque después se renombre o
--     elimine el producto. Nunca se borra.
--       motivo        : 'VENCIDO' (ya había vencido) o 'POR_VENCER' (se
--                       retiró antes de su fecha).
--       costo_unitario: costo promedio del producto si lo tiene (módulo de
--                       ganancias), si no su precio de compra; NULL si no
--                       tenía ninguno. Lo perdido = cantidad × costo.
--       fecha_hora    : hora de Lima, 'AAAA-MM-DD HH:MM:SS'.
--
-- Además, cada descarte deja un movimiento 'MERMA' en movimientos_inventario
-- (eso lo hace el backend, handlers/lotes.rs).

CREATE TABLE IF NOT EXISTS mermas_lote (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  lote_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  producto_nombre TEXT NOT NULL,
  unidad_medida TEXT NOT NULL DEFAULT 'UNIDAD',
  numero_lote TEXT,
  fecha_vencimiento TEXT NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad >= 0),
  costo_unitario REAL,
  motivo TEXT NOT NULL CHECK (motivo IN ('VENCIDO', 'POR_VENCER')),
  usuario_id INTEGER,
  usuario_nombre TEXT,
  fecha_hora TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mermas_lote_fecha ON mermas_lote (fecha_hora);
CREATE INDEX IF NOT EXISTS idx_mermas_lote_producto ON mermas_lote (producto_id);
