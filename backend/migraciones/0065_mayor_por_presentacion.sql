-- Precio por mayor de una presentación (módulos PRESENTACIONES + PRECIO_MAYOR).
--
-- Pedido de una licorería: la "Caja x12" de cerveza cuesta S/ 110, pero
-- desde 3 cajas cada una sale a S/ 100. La venta sigue igual que el six
-- pack: "3 × Caja x12", descuenta 36 unidades; solo cambia el precio de la
-- caja cuando la línea llega a `mayor_desde` cajas.
--
-- Solo ADITIVO: dos columnas nuevas, vacías para todas las presentaciones
-- que ya existen (sin ellas la presentación se vende como siempre).

ALTER TABLE producto_presentaciones ADD COLUMN mayor_desde REAL;
ALTER TABLE producto_presentaciones ADD COLUMN precio_mayor REAL;
