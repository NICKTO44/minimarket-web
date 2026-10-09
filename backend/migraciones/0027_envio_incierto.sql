-- Envíos a SUNAT que se cortaron sin respuesta (emisión directa).
--
-- Solo ADITIVO: una columna en cada tabla de documentos directos.
--
--   envio_incierto : 1 si algún envío del documento se cortó sin una
--     respuesta clara (sin conexión, error del servicio): pudo llegar a
--     SUNAT. Solo entonces, al reenviar:
--       - un "ya registrado" (1033) de una boleta o factura significa que el
--         documento es este (sin envío incierto, SUNAT tiene OTRO documento
--         con esa serie y número);
--       - una anulación que SUNAT rechaza por repetida no se da por
--         rechazada: hay que revisarla en SUNAT.

ALTER TABLE comprobantes_electronicos ADD COLUMN envio_incierto INTEGER NOT NULL DEFAULT 0;
ALTER TABLE notas_credito ADD COLUMN envio_incierto INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bajas_sunat ADD COLUMN envio_incierto INTEGER NOT NULL DEFAULT 0;
