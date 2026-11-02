-- Módulo TRES_PRECIOS (rubro Lubricentro / Repuestos): cada producto puede
-- tener, además de su precio de venta (Precio 1), un Precio 2 y un Precio 3
-- de referencia. En el punto de venta el vendedor elige cuál cobrar.
--
-- Solo ADITIVO: una tabla nueva. No toca productos, ventas ni stock. Un
-- producto sin fila aquí se vende solo a su precio, como siempre.
--
--   producto_precios_venta : una fila por producto con precios extra.
--     precio_2, precio_3 : NULL = no tiene ese precio.

CREATE TABLE IF NOT EXISTS producto_precios_venta (
  producto_id INTEGER PRIMARY KEY,
  precio_2 REAL,
  precio_3 REAL,
  fecha_actualizacion TEXT
);
