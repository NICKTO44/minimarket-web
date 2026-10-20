-- Módulos PRODUCCION e INSUMOS (rubro Pastelería / Panadería, y quien los
-- encienda): lo que se produce cada día, lo que se pierde (mermas) y las
-- recetas de producción con sus insumos (para saber cuánto cuesta hacer
-- cada producto). Ver handlers/produccion.rs.
--
-- Solo ADITIVO: tablas nuevas. El stock se mueve como siempre (columna
-- productos.stock, o lotes_producto si el producto lleva vencimiento) y
-- cada movimiento queda en movimientos_inventario (ENTRADA, SALIDA, MERMA).
--
--   formulas : la receta de producción de un producto (no confundir con
--              `recetas`, que es la receta médica de la farmacia).
--     rinde  : cuántas unidades del producto salen con esa receta
--              (una tanda de 50 panes, una torta).
--   formula_insumos : lo que lleva la receta, en la unidad del insumo.
--
--   producciones : cada vez que se registra lo producido.
--     costo_unitario / costo_total : de los insumos usados (NULL si no se
--                                    sabe el costo de alguno).
--     lotes  : solo si el producto lleva vencimiento, el lote que creó.
--     estado : REGISTRADA o ANULADA.
--   produccion_insumos : lo que se descontó de cada insumo.
--     faltante : lo que la receta pedía y no había en stock (se registró igual).
--     lotes    : "lote:cantidad,lote:cantidad" de donde salió (para anular).
--
--   mermas_registradas : lo que se pierde (no se vendió, se malogró, venció).
--     lotes : como en produccion_insumos.

CREATE TABLE IF NOT EXISTS formulas (
  producto_id INTEGER PRIMARY KEY,
  rinde REAL NOT NULL CHECK (rinde > 0),
  notas TEXT,
  fecha_actualizacion TEXT
);

CREATE TABLE IF NOT EXISTS formula_insumos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  insumo_id INTEGER NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0)
);

CREATE INDEX IF NOT EXISTS idx_formula_insumos_producto ON formula_insumos (producto_id);
CREATE INDEX IF NOT EXISTS idx_formula_insumos_insumo ON formula_insumos (insumo_id);

CREATE TABLE IF NOT EXISTS producciones (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  producto_nombre TEXT NOT NULL,
  unidad_medida TEXT,
  cantidad REAL NOT NULL CHECK (cantidad > 0),
  fecha_vencimiento TEXT,
  lote_id INTEGER,
  con_receta INTEGER NOT NULL DEFAULT 0,
  costo_unitario REAL,
  costo_total REAL,
  nota TEXT,
  estado TEXT NOT NULL DEFAULT 'REGISTRADA' CHECK (estado IN ('REGISTRADA', 'ANULADA')),
  usuario_id INTEGER,
  usuario_nombre TEXT,
  fecha_hora TEXT NOT NULL,
  anulada_por TEXT,
  fecha_anulacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_producciones_fecha ON producciones (fecha_hora);
CREATE INDEX IF NOT EXISTS idx_producciones_producto ON producciones (producto_id);

CREATE TABLE IF NOT EXISTS produccion_insumos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  produccion_id INTEGER NOT NULL,
  insumo_id INTEGER NOT NULL,
  insumo_nombre TEXT NOT NULL,
  unidad_medida TEXT,
  cantidad REAL NOT NULL,
  faltante REAL NOT NULL DEFAULT 0,
  costo_unitario REAL,
  lotes TEXT
);

CREATE INDEX IF NOT EXISTS idx_produccion_insumos_produccion ON produccion_insumos (produccion_id);

CREATE TABLE IF NOT EXISTS mermas_registradas (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  producto_nombre TEXT NOT NULL,
  unidad_medida TEXT,
  cantidad REAL NOT NULL CHECK (cantidad > 0),
  motivo TEXT NOT NULL CHECK (motivo IN ('NO_VENDIDO', 'MALOGRADO', 'VENCIDO', 'OTRO')),
  nota TEXT,
  costo_unitario REAL,
  lotes TEXT,
  estado TEXT NOT NULL DEFAULT 'REGISTRADA' CHECK (estado IN ('REGISTRADA', 'ANULADA')),
  usuario_id INTEGER,
  usuario_nombre TEXT,
  fecha_hora TEXT NOT NULL,
  anulada_por TEXT,
  fecha_anulacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_mermas_registradas_fecha ON mermas_registradas (fecha_hora);
