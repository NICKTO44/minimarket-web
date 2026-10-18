-- Módulo ORDENES_SERVICIO (rubro Lubricentro): el vehículo entra al taller,
-- se abre una orden, se le van agregando repuestos y mano de obra, se
-- asigna el mecánico y se cobra al terminar en el punto de venta.
--
-- Solo ADITIVO: tres tablas nuevas. No toca ventas ni stock: el stock se
-- descuenta recién al cobrar la orden (es una venta normal).
--
--   mecanicos : quienes trabajan en el taller y su comisión (% de la mano
--               de obra de las órdenes cobradas).
--
--   ordenes_servicio : una por vehículo que entra.
--     numero     : correlativo del negocio (Orden N.° 15).
--     estado     : ABIERTA (en trabajo), TERMINADA (lista para cobrar),
--                  COBRADA (tiene su venta) o ANULADA.
--     km, proximo_km, proxima_fecha : como en la venta con placa.
--     observaciones : lo que pidió el cliente / lo que se encontró.
--
--   orden_items : lo que se le pone.
--     precio_unitario : el del producto al agregarlo (se cobra ese).
--     descuento       : de un paquete (ver módulo PAQUETES).
--     es_servicio     : mano de obra (producto sin control de stock); es la
--                       base de la comisión del mecánico.

CREATE TABLE IF NOT EXISTS mecanicos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL,
  telefono TEXT,
  comision_porcentaje REAL NOT NULL DEFAULT 0,
  activo INTEGER NOT NULL DEFAULT 1,
  fecha_creacion TEXT
);

CREATE TABLE IF NOT EXISTS ordenes_servicio (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  numero INTEGER NOT NULL UNIQUE,
  vehiculo_id INTEGER NOT NULL,
  placa TEXT NOT NULL,
  cliente_id INTEGER,
  mecanico_id INTEGER,
  km INTEGER,
  proximo_km INTEGER,
  proxima_fecha TEXT,
  observaciones TEXT,
  estado TEXT NOT NULL DEFAULT 'ABIERTA' CHECK (estado IN ('ABIERTA', 'TERMINADA', 'COBRADA', 'ANULADA')),
  venta_id INTEGER,
  usuario_id INTEGER,
  motivo_anulacion TEXT,
  fecha_creacion TEXT,
  fecha_terminada TEXT,
  fecha_cobro TEXT
);

CREATE INDEX IF NOT EXISTS idx_ordenes_estado ON ordenes_servicio (estado);
CREATE INDEX IF NOT EXISTS idx_ordenes_vehiculo ON ordenes_servicio (vehiculo_id);
CREATE INDEX IF NOT EXISTS idx_ordenes_cobro ON ordenes_servicio (fecha_cobro);

CREATE TABLE IF NOT EXISTS orden_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  orden_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  nombre TEXT NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0),
  precio_unitario REAL NOT NULL CHECK (precio_unitario >= 0),
  descuento REAL NOT NULL DEFAULT 0 CHECK (descuento >= 0),
  es_servicio INTEGER NOT NULL DEFAULT 0,
  paquete_id INTEGER
);

CREATE INDEX IF NOT EXISTS idx_orden_items_orden ON orden_items (orden_id);
