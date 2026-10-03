-- Cotizaciones / proformas (módulo COTIZACIONES).
--
-- Solo ADITIVO: dos tablas nuevas. Una cotización guarda lo que se le
-- ofreció al cliente (productos, medidas, precios) sin tocar stock ni caja.
-- Después se carga en el punto de venta y, al cobrarla, queda VENDIDA.
--   estado: 'PENDIENTE' | 'VENDIDA' | 'ANULADA'
--   fecha : hora de Perú, calculada en el servidor (no 'localtime' de la base)

CREATE TABLE IF NOT EXISTS cotizaciones (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  numero INTEGER NOT NULL,
  cliente_id INTEGER,
  cliente_nombre TEXT,
  cliente_documento TEXT,
  total REAL NOT NULL DEFAULT 0,
  validez_dias INTEGER NOT NULL DEFAULT 7,
  notas TEXT,
  estado TEXT NOT NULL DEFAULT 'PENDIENTE',
  venta_id INTEGER,
  usuario_id INTEGER NOT NULL,
  fecha TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cotizacion_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  cotizacion_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  nombre TEXT NOT NULL,
  detalle TEXT,
  unidad_medida TEXT,
  cantidad REAL NOT NULL,
  precio_unitario REAL NOT NULL,
  total_linea REAL NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cotizaciones_estado ON cotizaciones(estado, id);
CREATE INDEX IF NOT EXISTS idx_cotizacion_items_cotizacion ON cotizacion_items(cotizacion_id);
