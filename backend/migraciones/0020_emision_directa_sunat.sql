-- Emisión directa a SUNAT (sin FacturaLibre), a través de Lycet.
--
-- Solo ADITIVO: una tabla y un índice nuevos, y columnas nuevas en
-- configuracion_tienda. Nada cambia para los negocios que siguen con
-- FacturaLibre: el modo se elige con configuracion_tienda.facturacion_proveedor
-- (columna que existe desde el schema original): 'SUNAT_DIRECTO' activa la
-- emisión directa; cualquier otro valor sigue como hasta hoy.
--
--   comprobante_archivos : por cada comprobante emitido directo, el
--     documento que se envió (JSON, para reenviarlo igual si SUNAT no
--     respondió), el XML firmado y la constancia de SUNAT (CDR, ZIP en
--     base64). Con FacturaLibre estos archivos vivían en su servidor; ahora
--     se guardan en la base del negocio y se respaldan con ella.
--
--   ux_comprobantes_directo_numero : en la emisión directa el número lo
--     pone el sistema, así que dos comprobantes jamás pueden repetir
--     serie y número (por ejemplo, dos cajeros emitiendo a la vez).
--     Solo aplica a los emitidos directo: los de FacturaLibre no se tocan.
--
--   configuracion_tienda.razon_social / departamento / provincia / distrito :
--     datos del emisor que SUNAT pide en cada comprobante y que FacturaLibre
--     tenía en su panel. La dirección y el ubigeo ya existían.
--
-- Las columnas de configuracion_tienda van al final a propósito: si existe
-- la última (distrito), todo lo anterior de este archivo ya se aplicó.

CREATE TABLE IF NOT EXISTS comprobante_archivos (
  comprobante_id INTEGER PRIMARY KEY,
  documento TEXT,
  xml TEXT,
  cdr_zip TEXT,
  actualizado TEXT,
  FOREIGN KEY (comprobante_id) REFERENCES comprobantes_electronicos(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_comprobantes_directo_numero
  ON comprobantes_electronicos(serie, numero)
  WHERE proveedor = 'SUNAT_DIRECTO';

ALTER TABLE configuracion_tienda ADD COLUMN razon_social TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN departamento TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN provincia TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN distrito TEXT;
