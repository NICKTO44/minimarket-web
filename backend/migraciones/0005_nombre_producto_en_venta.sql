-- Guarda en cada línea de venta el nombre y la unidad del producto TAL
-- COMO ERAN al momento de vender (como hacen los sistemas contables).
--
-- Antes, las ventas solo guardaban producto_id y el nombre se buscaba
-- en la tabla productos cada vez. Si alguien renombraba un producto,
-- las ventas antiguas (boleta pública, reimpresiones, devoluciones)
-- mostraban el nombre NUEVO, distinto del comprobante enviado a SUNAT.
--
-- Solo ALTER TABLE ADD COLUMN + UPDATE: no se reconstruye ninguna tabla
-- y no hay triggers sobre detalles_venta, así que no afecta a nada más.
-- Las ventas anteriores a esta migración se rellenan con el nombre y la
-- unidad actuales (es lo más fiel que existe para ellas).

ALTER TABLE detalles_venta ADD COLUMN nombre_producto TEXT;
ALTER TABLE detalles_venta ADD COLUMN unidad_medida TEXT;

UPDATE detalles_venta
SET
  nombre_producto = (SELECT p.nombre FROM productos p WHERE p.id = detalles_venta.producto_id),
  unidad_medida   = (SELECT p.unidad_medida FROM productos p WHERE p.id = detalles_venta.producto_id)
WHERE nombre_producto IS NULL;
