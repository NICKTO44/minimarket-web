-- Módulo Gastos: lo que el negocio paga y que no es mercadería (alquiler,
-- luz, sueldos, movilidad...). La mercadería sigue por Compras, para no
-- contarla dos veces.
--
-- Solo ADITIVO: dos tablas nuevas y sus categorías de ejemplo.
--
--   categorias_gasto : categorías del negocio (trae unas listas para usar;
--     el dueño puede agregar las suyas o desactivarlas).
--
--   gastos : un registro por gasto. Nunca se borra: un error se ANULA con
--     su motivo. Si se pagó con efectivo de la caja abierta (metodo_pago =
--     'EFECTIVO_CAJA'), queda enlazado a esa caja y a su movimiento en
--     movimientos_caja, así el efectivo esperado al cerrar ya lo descuenta.
--     fecha es el día del gasto en Perú (AAAA-MM-DD), elegido por quien lo
--     registra; registrado es cuándo se anotó en el sistema.

CREATE TABLE IF NOT EXISTS categorias_gasto (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL UNIQUE,
  activo INTEGER NOT NULL DEFAULT 1,
  orden INTEGER NOT NULL DEFAULT 100
);

INSERT OR IGNORE INTO categorias_gasto (nombre, orden) VALUES
  ('Alquiler', 10),
  ('Luz, agua e internet', 20),
  ('Sueldos y pagos al personal', 30),
  ('Transporte y movilidad', 40),
  ('Insumos y útiles', 50),
  ('Impuestos y trámites', 60),
  ('Mantenimiento y reparaciones', 70),
  ('Publicidad', 80),
  ('Otros', 999);

CREATE TABLE IF NOT EXISTS gastos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  fecha TEXT NOT NULL,
  categoria_id INTEGER NOT NULL,
  descripcion TEXT NOT NULL,
  monto REAL NOT NULL CHECK (monto > 0),
  metodo_pago TEXT NOT NULL CHECK (metodo_pago IN
    ('EFECTIVO_CAJA', 'EFECTIVO', 'YAPE', 'PLIN', 'TRANSFERENCIA', 'TARJETA', 'OTRO')),
  pagado_a TEXT,
  comprobante_tipo TEXT CHECK (comprobante_tipo IS NULL OR comprobante_tipo IN
    ('BOLETA', 'FACTURA', 'RECIBO', 'TICKET', 'OTRO')),
  comprobante_numero TEXT,
  caja_id INTEGER,
  movimiento_caja_id INTEGER,
  usuario_id INTEGER NOT NULL,
  registrado TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  anulado INTEGER NOT NULL DEFAULT 0,
  anulado_por INTEGER,
  motivo_anulacion TEXT,
  fecha_anulacion TEXT,
  FOREIGN KEY (categoria_id) REFERENCES categorias_gasto(id),
  FOREIGN KEY (caja_id) REFERENCES cajas(id),
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id)
);

CREATE INDEX IF NOT EXISTS idx_gastos_fecha ON gastos(fecha);
CREATE INDEX IF NOT EXISTS idx_gastos_categoria ON gastos(categoria_id);
CREATE INDEX IF NOT EXISTS idx_gastos_caja ON gastos(caja_id);
