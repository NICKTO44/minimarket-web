-- Módulo VENTA_FUERA_INVENTARIO: vender algo que el negocio consigue afuera
-- y no está en su inventario (handlers/fuera_inventario.rs).
--
-- Solo ADITIVO: dos tablas nuevas. No toca ventas, detalles_venta, cajas,
-- créditos ni ningún trigger. Lo de afuera NO entra a la venta del sistema
-- (su total, su pago, la caja, reportes, ganancias, stock y créditos son
-- solo del inventario): solo existe aquí y sale únicamente en el
-- comprobante que se entrega al cliente y se envía a SUNAT.
--
--   venta_fuera_inventario : una fila por venta que llevó algo de afuera.
--     total         : suma de sus líneas (con IGV).
--     pago_efectivo, pago_otro : el pago mixto TAL COMO lo hizo el cliente
--                     (por el total del comprobante), solo para imprimirlo.
--                     En la venta queda solo la parte del inventario.
--
--   venta_fuera_inventario_linea : lo que se vendió de afuera, en el orden
--     en que va en el comprobante (después de las líneas del inventario).

CREATE TABLE IF NOT EXISTS venta_fuera_inventario (
  venta_id INTEGER PRIMARY KEY,
  total REAL NOT NULL,
  pago_efectivo REAL,
  pago_otro REAL
);

CREATE TABLE IF NOT EXISTS venta_fuera_inventario_linea (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_id INTEGER NOT NULL,
  nombre TEXT NOT NULL,
  cantidad REAL NOT NULL,
  precio_unitario REAL NOT NULL,
  total_linea REAL NOT NULL,
  unidad_medida TEXT NOT NULL DEFAULT 'UNIDAD',
  codigo TEXT
);

CREATE INDEX IF NOT EXISTS idx_fuera_inventario_linea_venta ON venta_fuera_inventario_linea (venta_id);
