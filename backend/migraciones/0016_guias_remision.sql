-- Guía de remisión remitente electrónica por FacturaLibre (módulo GUIAS).
--
-- Solo ADITIVO: una tabla nueva y tres columnas en configuracion_tienda.
--   guias_remision.estado: 'REGISTRADA' (creada en FacturaLibre)
--                        | 'ENVIADA'    (enviada a SUNAT, esperando respuesta)
--                        | 'ACEPTADA' | 'RECHAZADA' | 'ERROR' (no se pudo crear)
--   guias_remision.datos_json: el formulario completo (transporte,
--     direcciones, ítems), para mostrarla y reintentar.
--   configuracion_tienda.serie_guia  : serie de guías (por defecto T001)
--   configuracion_tienda.ubigeo      : ubigeo (INEI, 6 dígitos) del local = punto de partida
--   configuracion_tienda.guia_ultimo : últimos datos de transporte usados, para no volver a escribirlos

CREATE TABLE IF NOT EXISTS guias_remision (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_id INTEGER,
  serie TEXT,
  numero INTEGER,
  external_id TEXT,
  estado TEXT NOT NULL DEFAULT 'REGISTRADA',
  mensaje TEXT,
  enlace_pdf TEXT,
  enlace_xml TEXT,
  enlace_cdr TEXT,
  destinatario_nombre TEXT,
  destinatario_documento TEXT,
  llegada_direccion TEXT,
  fecha_traslado TEXT,
  datos_json TEXT,
  usuario_id INTEGER NOT NULL,
  fecha TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_guias_venta ON guias_remision(venta_id);

ALTER TABLE configuracion_tienda ADD COLUMN serie_guia TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN ubigeo TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN guia_ultimo TEXT;
