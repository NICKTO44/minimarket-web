-- Anulación de comprobantes emitidos directo a SUNAT.
--
-- Solo ADITIVO: una tabla nueva y una columna en comprobantes_electronicos.
--
--   bajas_sunat : cada pedido de anulación enviado a SUNAT, con su
--     identificador, su ticket y la constancia.
--       tipo = 'BAJA'    -> comunicación de baja (RA-AAAAMMDD-n), facturas.
--       tipo = 'RESUMEN' -> resumen diario con estado 3 "anulado"
--                           (RC-AAAAMMDD-n), boletas.
--     SUNAT responde con un ticket y procesa después: estado PENDIENTE
--     hasta que el ticket da la constancia (ACEPTADO o RECHAZADO). Sin
--     ticket (no hubo conexión) se vuelve a enviar el mismo documento.
--     devolucion_id: la devolución con la que se regresó el stock y el
--     dinero de la venta al anular.
--
--   comprobantes_electronicos.anulacion : NULL (vigente), 'EN_PROCESO'
--     (se pidió la anulación y SUNAT aún no responde), 'ANULADO' o
--     'RECHAZADA' (SUNAT no aceptó la baja: el comprobante sigue vigente).

CREATE TABLE IF NOT EXISTS bajas_sunat (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tipo TEXT NOT NULL,
  identificador TEXT NOT NULL,
  comprobante_id INTEGER NOT NULL,
  fecha_documento TEXT NOT NULL,
  motivo TEXT NOT NULL,
  ticket TEXT,
  estado TEXT NOT NULL DEFAULT 'PENDIENTE',
  mensaje TEXT,
  documento TEXT,
  xml TEXT,
  cdr_zip TEXT,
  devolucion_id INTEGER,
  usuario_id INTEGER,
  fecha TEXT NOT NULL,
  intentos INTEGER NOT NULL DEFAULT 0,
  ultimo_intento TEXT,
  FOREIGN KEY (comprobante_id) REFERENCES comprobantes_electronicos(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_bajas_sunat_identificador ON bajas_sunat(identificador);
CREATE INDEX IF NOT EXISTS ix_bajas_sunat_estado ON bajas_sunat(estado);

ALTER TABLE comprobantes_electronicos ADD COLUMN anulacion TEXT;
