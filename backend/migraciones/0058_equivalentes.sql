-- Módulo EQUIVALENTES (rubro Lubricentro / Repuestos): productos que el
-- dueño marca como equivalentes (resuelven lo mismo). Los elige él a mano.
--
-- Solo ADITIVO: una tabla nueva. No toca productos, ventas ni stock. Un
-- producto sin fila aquí no tiene equivalentes.
--
--   producto_equivalente : una fila por producto que está en un grupo.
--     grupo : los productos con el mismo grupo son equivalentes entre sí
--             (en los dos sentidos y en cadena). Es el id del producto más
--             chico del grupo; un grupo tiene siempre 2 o más productos.

CREATE TABLE IF NOT EXISTS producto_equivalente (
  producto_id INTEGER PRIMARY KEY,
  grupo INTEGER NOT NULL,
  fecha_actualizacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_equivalente_grupo ON producto_equivalente (grupo);
