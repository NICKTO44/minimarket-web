-- Ventas al crédito y abonos (módulo CREDITO).
--
-- Solo ADITIVO: dos tablas nuevas; no se toca la tabla ventas.
-- Una venta al crédito se guarda como venta normal con
--   metodo_pago = 'MIXTO', pago_efectivo = 0, pago_otro = total,
--   pago_otro_metodo = 'CREDITO'
-- (el CHECK de ventas.metodo_pago no admite un valor nuevo y no se puede
-- cambiar sin recrear la tabla). Así los triggers de caja de la migración
-- 0004 suman la venta al total vendido pero NO al efectivo, la tarjeta ni
-- la transferencia: el dinero entra recién con cada abono.
--   creditos.saldo = total - abonado - devuelto
--   estado: 'PENDIENTE' | 'PAGADO'
--   Un abono en EFECTIVO entra a la caja abierta como movimiento INGRESO.

CREATE TABLE IF NOT EXISTS creditos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_id INTEGER NOT NULL UNIQUE,
  cliente_id INTEGER NOT NULL,
  total REAL NOT NULL,
  abonado REAL NOT NULL DEFAULT 0,
  devuelto REAL NOT NULL DEFAULT 0,
  vence TEXT,
  estado TEXT NOT NULL DEFAULT 'PENDIENTE',
  usuario_id INTEGER NOT NULL,
  fecha TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS credito_abonos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  credito_id INTEGER NOT NULL,
  monto REAL NOT NULL,
  metodo_pago TEXT NOT NULL,
  caja_id INTEGER,
  nota TEXT,
  usuario_id INTEGER NOT NULL,
  fecha TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_creditos_cliente ON creditos(cliente_id, estado);
CREATE INDEX IF NOT EXISTS idx_creditos_estado ON creditos(estado, id);
CREATE INDEX IF NOT EXISTS idx_credito_abonos_credito ON credito_abonos(credito_id);
