-- Datos del alta en emisión directa, para verlos en el panel de Monspeet.
--
-- Solo ADITIVO: columnas nuevas en configuracion_tienda, vacías hasta que el
-- negocio se da de alta desde el panel (o con `sunat alta`). Nada de esto se
-- usa para emitir: la clave SOL y el certificado viven solo en Lycet. Aquí
-- queda lo que sirve para mostrar y avisar:
--
--   sunat_usuario_sol  : usuario secundario SOL (sin el RUC delante, sin clave)
--   sunat_ambiente     : 'BETA' (pruebas) o 'PRODUCCION'
--   sunat_cert_titular : a nombre de quién está el certificado
--   sunat_cert_vence   : último día de validez del certificado (AAAA-MM-DD)
--   sunat_cert_serie   : número de serie, para comprobar que Lycet firma con él
--   sunat_alta_fecha   : cuándo se registró en Lycet por última vez
--
-- La última columna (sunat_alta_fecha) marca que el archivo ya se aplicó.

ALTER TABLE configuracion_tienda ADD COLUMN sunat_usuario_sol TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN sunat_ambiente TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN sunat_cert_titular TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN sunat_cert_vence TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN sunat_cert_serie TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN sunat_alta_fecha TEXT;
