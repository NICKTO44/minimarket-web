-- Notas de crédito electrónicas (tipo 07) de la emisión directa a SUNAT.
--
-- Solo ADITIVO: una tabla nueva y dos columnas en configuracion_tienda.
--
--   notas_credito : cada nota corrige UN comprobante (boleta o factura)
--     aceptado por SUNAT: lo anula (motivo 01), lo devuelve completo (06)
--     o devuelve algunas prendas (07). Como en las boletas y facturas
--     directas, el número se reserva antes de enviar (índice único por
--     serie y número) y el documento enviado se guarda tal cual para
--     reenviarlo igual si SUNAT no responde. El XML firmado y la
--     constancia (CDR) también quedan aquí.
--     devolucion_id une la nota con la devolución o el cambio de prenda
--     que la originó (NULL si se emitió desde Comprobantes).
--
--   configuracion_tienda.serie_nc_boleta / serie_nc_factura : series de
--     las notas. SUNAT exige que empiecen con la letra del comprobante que
--     corrigen: B para boletas, F para facturas.
--
-- Las columnas de configuracion_tienda van al final: si existe la última,
-- todo lo anterior de este archivo ya se aplicó.

CREATE TABLE IF NOT EXISTS notas_credito (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  comprobante_id INTEGER NOT NULL,
  venta_id INTEGER NOT NULL,
  devolucion_id INTEGER,
  serie TEXT NOT NULL,
  numero INTEGER NOT NULL,
  motivo_codigo TEXT NOT NULL,
  motivo TEXT NOT NULL,
  total REAL NOT NULL,
  estado TEXT NOT NULL DEFAULT 'PENDIENTE',
  mensaje_sunat TEXT,
  hash TEXT,
  documento TEXT,
  xml TEXT,
  cdr_zip TEXT,
  intentos INTEGER NOT NULL DEFAULT 0,
  ultimo_intento TEXT,
  usuario_id INTEGER,
  fecha_emision TEXT NOT NULL,
  FOREIGN KEY (comprobante_id) REFERENCES comprobantes_electronicos(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_notas_credito_numero ON notas_credito(serie, numero);
CREATE INDEX IF NOT EXISTS ix_notas_credito_comprobante ON notas_credito(comprobante_id);
CREATE INDEX IF NOT EXISTS ix_notas_credito_estado ON notas_credito(estado);

ALTER TABLE configuracion_tienda ADD COLUMN serie_nc_boleta TEXT DEFAULT 'BC01';
ALTER TABLE configuracion_tienda ADD COLUMN serie_nc_factura TEXT DEFAULT 'FC01';
