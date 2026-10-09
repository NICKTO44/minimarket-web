-- Guías de remisión emitidas directo a SUNAT (sin FacturaLibre), por la
-- API de guías de SUNAT a través de Lycet.
--
-- Solo ADITIVO: columnas nuevas en guias_remision y configuracion_tienda y
-- un índice nuevo. Las guías de FacturaLibre siguen igual (proveedor NULL).
--
--   guias_remision.proveedor : 'SUNAT_DIRECTO' en las directas.
--   guias_remision.ticket    : SUNAT recibe la guía y la procesa después;
--                              la constancia se pide con este ticket.
--   guias_remision.documento / xml / cdr_zip : la guía tal como se envió,
--                              el XML firmado y la constancia de SUNAT.
--   guias_remision.intentos / ultimo_intento : reenvíos automáticos.
--   ux_guias_directo_numero  : en las directas el número lo pone el
--                              sistema; nunca se repite serie y número.
--   configuracion_tienda.sunat_gre_client_id : el ID de las credenciales
--                              API de SUNAT del negocio (el secreto vive
--                              solo en Lycet), para mostrar en el panel.
--
-- La columna de configuracion_tienda va al final: si existe, todo lo
-- anterior de este archivo ya se aplicó.

ALTER TABLE guias_remision ADD COLUMN proveedor TEXT;
ALTER TABLE guias_remision ADD COLUMN ticket TEXT;
ALTER TABLE guias_remision ADD COLUMN documento TEXT;
ALTER TABLE guias_remision ADD COLUMN xml TEXT;
ALTER TABLE guias_remision ADD COLUMN cdr_zip TEXT;
ALTER TABLE guias_remision ADD COLUMN intentos INTEGER NOT NULL DEFAULT 0;
ALTER TABLE guias_remision ADD COLUMN ultimo_intento TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS ux_guias_directo_numero
  ON guias_remision(serie, numero)
  WHERE proveedor = 'SUNAT_DIRECTO';

ALTER TABLE configuracion_tienda ADD COLUMN sunat_gre_client_id TEXT;
