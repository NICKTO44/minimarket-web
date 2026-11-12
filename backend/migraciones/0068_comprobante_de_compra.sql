-- Comprobante que entregó el proveedor en cada compra (opcional):
-- FACTURA, BOLETA o NOTA_VENTA. La serie y el número van en factura_numero
-- (ya existía). Compras anteriores quedan con NULL: "sin comprobante".
ALTER TABLE compras ADD COLUMN tipo_comprobante TEXT;
