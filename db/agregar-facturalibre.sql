ALTER TABLE configuracion_tienda ADD COLUMN facturalibre_token TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN facturalibre_ruta TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN codigo_producto_sunat_generico TEXT DEFAULT '50000000';

ALTER TABLE comprobantes_electronicos ADD COLUMN enlace_cdr TEXT;
ALTER TABLE comprobantes_electronicos ADD COLUMN external_id TEXT;
ALTER TABLE comprobantes_electronicos ADD COLUMN hash TEXT;
