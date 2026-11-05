-- Establecimiento anexo del emisor (Panel → Facturación).
--
-- Un RUC puede tener locales registrados en SUNAT como "establecimientos
-- anexos" (Ficha RUC), cada uno con un código de 4 dígitos; el domicilio
-- fiscal es el 0000. Si el negocio vende desde un anexo, sus facturas,
-- boletas, notas de crédito y guías salen con ese código (AddressTypeCode
-- del emisor) y en la impresión va "Sucursal: <dirección>" antes de la
-- dirección fiscal.
--
-- Solo ADITIVO: dos columnas nuevas. Por defecto '0000' = como siempre; los
-- negocios que no lo configuren no cambian en nada.
--
--   codigo_establecimiento   : '0000' (domicilio fiscal) o el código del anexo.
--   direccion_establecimiento: dirección del anexo para imprimir.

ALTER TABLE configuracion_tienda ADD COLUMN direccion_establecimiento TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN codigo_establecimiento TEXT NOT NULL DEFAULT '0000';
