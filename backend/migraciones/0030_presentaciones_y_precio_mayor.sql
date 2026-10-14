-- Ferretería (y cualquier negocio que lo encienda): un mismo producto se
-- vende de varias formas, y con precio especial por cantidad.
--
-- Solo ADITIVO: dos tablas nuevas. No toca productos, ventas ni stock.
--
--   producto_presentaciones : formas de vender un producto además de su
--     unidad (cable por METRO -> "Rollo x 100 m"). `factor` es cuántas
--     unidades del producto lleva una presentación (100) y `precio`, lo que
--     cuesta la presentación entera (S/ 85). El stock sigue en la unidad del
--     producto: vender 1 rollo descuenta 100 m. En la venta queda como 100 m
--     a S/ 0.85 (precio / factor), con el nombre "Cable (1 × Rollo x 100 m)".
--
--   producto_precios_mayor : precio por unidad desde cierta cantidad
--     ("desde 50 und: S/ 0.80"). El punto de venta lo aplica solo y el
--     servidor lo acepta como precio válido de esa línea.

CREATE TABLE IF NOT EXISTS producto_presentaciones (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  nombre TEXT NOT NULL,
  factor REAL NOT NULL CHECK (factor > 0),
  precio REAL NOT NULL CHECK (precio > 0),
  orden INTEGER NOT NULL DEFAULT 0,
  activo INTEGER NOT NULL DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_presentaciones_producto ON producto_presentaciones (producto_id);

CREATE TABLE IF NOT EXISTS producto_precios_mayor (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  desde_cantidad REAL NOT NULL CHECK (desde_cantidad > 0),
  precio REAL NOT NULL CHECK (precio > 0)
);

CREATE INDEX IF NOT EXISTS idx_precios_mayor_producto ON producto_precios_mayor (producto_id);
