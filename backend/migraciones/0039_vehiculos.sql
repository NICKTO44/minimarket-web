-- Módulo VEHICULOS (rubro Lubricentro): los vehículos de los clientes, la
-- placa y el kilometraje de cada venta, y el próximo servicio.
--
-- Solo ADITIVO: dos tablas nuevas. No toca ventas, clientes ni stock: la
-- placa de una venta vive aparte (venta_vehiculo), así una venta sin placa
-- es exactamente la de siempre.
--
--   vehiculos : uno por placa.
--     placa        : como se muestra ("ABC-123").
--     placa_clave  : solo letras y números, en mayúsculas ("ABC123"); no se
--                    repite en el negocio.
--     cliente_id   : el dueño (opcional).
--     km_ultimo    : el kilometraje más alto anotado.
--
--   venta_vehiculo : la placa y el kilometraje con que se hizo una venta.
--     placa                    : congelada al vender (por si se corrige luego).
--     km                       : kilometraje al llegar.
--     proximo_km, proxima_fecha : próximo servicio (sale en el ticket y en
--                                los recordatorios).

CREATE TABLE IF NOT EXISTS vehiculos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  placa TEXT NOT NULL,
  placa_clave TEXT NOT NULL UNIQUE,
  cliente_id INTEGER,
  marca TEXT,
  modelo TEXT,
  anio INTEGER,
  motor TEXT,
  color TEXT,
  notas TEXT,
  km_ultimo INTEGER,
  activo INTEGER NOT NULL DEFAULT 1,
  fecha_creacion TEXT,
  fecha_actualizacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_vehiculos_cliente ON vehiculos (cliente_id);

CREATE TABLE IF NOT EXISTS venta_vehiculo (
  venta_id INTEGER PRIMARY KEY,
  vehiculo_id INTEGER NOT NULL,
  placa TEXT NOT NULL,
  km INTEGER,
  proximo_km INTEGER,
  proxima_fecha TEXT,
  observacion TEXT,
  fecha TEXT
);

CREATE INDEX IF NOT EXISTS idx_venta_vehiculo_vehiculo ON venta_vehiculo (vehiculo_id);
CREATE INDEX IF NOT EXISTS idx_venta_vehiculo_proxima ON venta_vehiculo (proxima_fecha);
