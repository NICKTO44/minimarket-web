-- Tienda de ropa y calzado: tallas y colores (módulo VARIANTES) y cambio de
-- prenda (módulo CAMBIOS).
--
-- Solo ADITIVO: cuatro columnas en productos, una en configuracion_tienda y
-- una tabla nueva. Un negocio sin esos módulos no las usa: sus productos
-- quedan con las cuatro columnas en NULL y todo se ve igual que antes.
--
-- Cada talla/color de un modelo es un PRODUCTO normal, con su propio código
-- de barras, precio y stock, así ventas, compras, devoluciones, reportes y
-- comprobantes funcionan sin cambios. Lo que los une es modelo_id:
--   productos.modelo_id     : número del modelo (las tallas de un mismo polo comparten el mismo)
--   productos.modelo_nombre : nombre del modelo sin talla ni color ("Polo básico")
--   productos.talla         : "M", "32", "38"...
--   productos.color         : "Negro"...
--   productos.nombre sigue siendo el nombre completo que sale en la venta y
--     en la boleta: "Polo básico · M · Negro".
--
--   configuracion_tienda.cambio_dias : días que el cliente tiene para cambiar
--     una prenda (NULL = 7; 0 = sin plazo). Se imprime en el ticket.
--
--   cambios: une la venta original, la devolución de lo que el cliente trajo
--     y la venta de lo que se llevó.

ALTER TABLE productos ADD COLUMN modelo_id INTEGER;
ALTER TABLE productos ADD COLUMN modelo_nombre TEXT;
ALTER TABLE productos ADD COLUMN talla TEXT;
ALTER TABLE productos ADD COLUMN color TEXT;

CREATE INDEX IF NOT EXISTS idx_productos_modelo ON productos(modelo_id);

ALTER TABLE configuracion_tienda ADD COLUMN cambio_dias INTEGER;

CREATE TABLE IF NOT EXISTS cambios (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_original_id INTEGER NOT NULL,
  venta_nueva_id INTEGER NOT NULL,
  devolucion_id INTEGER NOT NULL,
  valor_devuelto REAL NOT NULL,
  total_nuevo REAL NOT NULL,
  -- total_nuevo - valor_devuelto: positivo = el cliente pagó la diferencia;
  -- negativo = se le devolvió.
  diferencia REAL NOT NULL,
  metodo TEXT NOT NULL,
  usuario_id INTEGER NOT NULL,
  fecha TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cambios_venta_original ON cambios(venta_original_id);
CREATE INDEX IF NOT EXISTS idx_cambios_venta_nueva ON cambios(venta_nueva_id);
