-- Módulo "Cargos rápidos" (CARGOS_RAPIDOS): productos que salen como botón
-- al cobrar (vaso, bolsa, servicio). gratis_por_venta: cuántos van gratis en
-- cada venta (el resto se cobra). auto_desde / auto_hasta ("HH:MM"): si hay
-- horario, se agrega solo una vez por venta dentro de ese horario.
CREATE TABLE IF NOT EXISTS cargos_rapidos (
  producto_id INTEGER PRIMARY KEY,
  orden INTEGER NOT NULL DEFAULT 0,
  gratis_por_venta REAL NOT NULL DEFAULT 0,
  auto_desde TEXT,
  auto_hasta TEXT
);
