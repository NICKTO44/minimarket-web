-- Módulo COMPATIBILIDAD (rubro Lubricentro / Repuestos): con qué vehículos
-- es compatible cada repuesto, para buscar "Hilux 2015 -> filtros" en el
-- punto de venta.
--
-- Solo ADITIVO: una tabla nueva. No toca productos, ventas ni stock.
--
--   producto_compatibilidad : una fila por vehículo con el que va el producto.
--     marca, modelo          : como se escribieron ("Toyota", "Hilux").
--     marca_clave, modelo_clave : en minúsculas, sin tildes ni espacios de
--                              más, para buscar.
--     anio_desde, anio_hasta : años del modelo (los dos NULL = todos; solo
--                              desde = de ese año en adelante).
--     motor                  : "2.5 D-4D", "1KD-FTV", "150 cc".
--     nota                   : algo más ("solo 4x4", "delantero").

CREATE TABLE IF NOT EXISTS producto_compatibilidad (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  marca TEXT NOT NULL,
  marca_clave TEXT NOT NULL,
  modelo TEXT,
  modelo_clave TEXT,
  anio_desde INTEGER,
  anio_hasta INTEGER,
  motor TEXT,
  nota TEXT
);

CREATE INDEX IF NOT EXISTS idx_compatibilidad_producto ON producto_compatibilidad (producto_id);
CREATE INDEX IF NOT EXISTS idx_compatibilidad_vehiculo ON producto_compatibilidad (marca_clave, modelo_clave);
