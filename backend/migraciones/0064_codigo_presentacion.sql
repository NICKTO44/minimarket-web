-- Código de barras de una presentación (módulo PRESENTACIONES).
--
-- Pedido de una licorería que migra de otro sistema: el six pack y la caja
-- traen su propio código de barras. Al escanearlo, en el punto de venta
-- entra directo esa presentación (S/ 35, descuenta 6 unidades).
--
-- Solo ADITIVO: una columna nueva, vacía para todas las presentaciones que
-- ya existen (los negocios que no la usan no cambian en nada).

ALTER TABLE producto_presentaciones ADD COLUMN codigo_barras TEXT;
CREATE INDEX IF NOT EXISTS idx_presentaciones_codigo ON producto_presentaciones (codigo_barras);
