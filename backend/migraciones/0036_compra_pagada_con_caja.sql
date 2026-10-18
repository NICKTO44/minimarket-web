-- Compras pagadas con el efectivo de la caja abierta.
--
-- Solo ADITIVO: dos columnas en compras. Cuando una compra se paga con la
-- plata de la caja, se registra un movimiento GASTO en movimientos_caja
-- (así el efectivo esperado al cerrar ya lo descuenta) y la compra queda
-- enlazada a esa caja y a ese movimiento. Las compras pagadas de otro modo
-- (efectivo de otro lado, transferencia, crédito) dejan ambas en NULL.

ALTER TABLE compras ADD COLUMN caja_id INTEGER;
ALTER TABLE compras ADD COLUMN movimiento_caja_id INTEGER;
