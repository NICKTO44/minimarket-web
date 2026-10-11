-- Pago mixto entre dos medios cualesquiera (p. ej. Yape + Plin).
--
-- Solo ADITIVO: una columna. Los triggers de caja que la usan están en la
-- migración 0029 (separados para que este archivo se pueda reintentar si
-- se corta a mitad).
--
--   ventas.pago_efectivo_metodo : con qué se pagó la parte guardada en
--     `pago_efectivo` de una venta MIXTO. NULL = efectivo (todas las
--     ventas anteriores y el mixto de siempre). Si no: 'TARJETA',
--     'TRANSFERENCIA', 'YAPE' o 'PLIN'.
--     La otra parte sigue en `pago_otro` / `pago_otro_metodo` (con Yape o
--     Plin guardados como 'YAPE_PLIN' y la billetera en `ventas.billetera`).

ALTER TABLE ventas ADD COLUMN pago_efectivo_metodo TEXT;
