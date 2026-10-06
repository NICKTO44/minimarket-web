-- Yape y Plin por separado (antes eran un solo medio, "Yape/Plin").
--
-- Solo ADITIVO: columnas nuevas. No se toca el CHECK de ventas.metodo_pago
-- (cambiarlo obligaría a recrear la tabla) ni los triggers de caja.
--
--   ventas.billetera : 'YAPE' o 'PLIN'. La venta se sigue guardando con
--     metodo_pago = 'YAPE_PLIN' (o MIXTO con pago_otro_metodo = 'YAPE_PLIN'),
--     como siempre; esta columna dice cuál de las dos fue.
--     NULL = venta anterior a este cambio, o que no fue por Yape ni Plin.
--
--   cajas.ventas_yape / ventas_plin / ventas_yape_plin : lo cobrado en esa
--     caja por Yape, por Plin y por "Yape/Plin" sin separar (ventas hechas
--     desde una pantalla que aún no distinguía). Los tres ya están dentro
--     de ventas_transferencia, que los triggers siguen llevando igual:
--       transferencia sola = ventas_transferencia - yape - plin - yape_plin
--     Los suma el servidor al registrar la venta (no un trigger).
--
--   cajas.detalle_billeteras : 1 si la caja se abrió con este cambio ya
--     instalado (su "transferencia sola" es exacta). 0 = caja anterior: ahí
--     transferencia, Yape y Plin estaban juntos y no se pueden separar.
--
-- La columna de ventas va al final a propósito: si existe, todo lo anterior
-- de este archivo ya se aplicó.

ALTER TABLE cajas ADD COLUMN ventas_yape REAL NOT NULL DEFAULT 0;
ALTER TABLE cajas ADD COLUMN ventas_plin REAL NOT NULL DEFAULT 0;
ALTER TABLE cajas ADD COLUMN ventas_yape_plin REAL NOT NULL DEFAULT 0;
ALTER TABLE cajas ADD COLUMN detalle_billeteras INTEGER NOT NULL DEFAULT 0;

ALTER TABLE ventas ADD COLUMN billetera TEXT;
