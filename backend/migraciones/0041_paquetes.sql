-- Módulo PAQUETES: varios productos a un solo precio ("Cambio de aceite
-- completo": aceite + filtro + mano de obra). Pensado para el lubricentro.
--
-- Solo ADITIVO: dos tablas nuevas. No toca productos, ventas ni stock. Al
-- vender un paquete entra cada producto en su línea con su precio de
-- siempre y un descuento por línea hasta llegar al precio del paquete; el
-- stock se descuenta de cada producto como en cualquier venta.
--
--   paquetes      : nombre y precio del paquete.
--   paquete_items : qué lleva y cuánto de cada producto.

CREATE TABLE IF NOT EXISTS paquetes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL,
  precio REAL NOT NULL CHECK (precio > 0),
  activo INTEGER NOT NULL DEFAULT 1,
  fecha_creacion TEXT,
  fecha_actualizacion TEXT
);

CREATE TABLE IF NOT EXISTS paquete_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  paquete_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0)
);

CREATE INDEX IF NOT EXISTS idx_paquete_items_paquete ON paquete_items (paquete_id);
