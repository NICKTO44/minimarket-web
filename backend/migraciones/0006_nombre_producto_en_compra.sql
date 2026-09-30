-- Igual que 0005 pero para las compras a proveedores: cada línea de
-- compra guarda el nombre del producto tal como era al registrar la
-- compra, para que el historial de compras no cambie si luego se
-- renombra el producto.
--
-- Solo ALTER TABLE ADD COLUMN + UPDATE; no hay triggers sobre
-- detalles_compra (los de compras son sobre la tabla compras).

ALTER TABLE detalles_compra ADD COLUMN nombre_producto TEXT;

UPDATE detalles_compra
SET nombre_producto = (SELECT p.nombre FROM productos p WHERE p.id = detalles_compra.producto_id)
WHERE nombre_producto IS NULL;
