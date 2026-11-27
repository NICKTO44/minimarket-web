-- Con qué se pagó una compra, con más detalle que tipo_pago (que tiene un
-- CHECK: EFECTIVO, TRANSFERENCIA, CREDITO, MIXTO). Lo llena la Compra por
-- peso: EFECTIVO_CAJA (de la caja abierta), EFECTIVO (de otro lado), YAPE,
-- PLIN o TRANSFERENCIA. En las demás compras queda NULL y Reportes lo deduce
-- (con caja_id = efectivo de la caja; si no, su tipo_pago).
--
-- Solo ADITIVO: una columna sin valor por defecto.
ALTER TABLE compras ADD COLUMN medio_pago TEXT;
