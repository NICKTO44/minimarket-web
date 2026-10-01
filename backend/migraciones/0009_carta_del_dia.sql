-- Módulo Cafetería / Restaurante: "Carta de hoy".
--
-- Solo ADITIVO (se aplica a todos los negocios, existentes y nuevos; en
-- los que están en modo Tienda no cambia nada: ambas columnas quedan
-- vacías / en 0 para todos sus productos).
--
-- Cada plato del día es un producto normal (así pedidos, comandas, cobro,
-- caja y reportes funcionan igual) con:
--   carta_fecha = 'AAAA-MM-DD' -> solo se lista ese día; al siguiente
--                                 desaparece solo de mesas, POS e inventario.
--   agotado     = 1            -> se ve en gris y no se puede pedir.
-- Los productos de siempre tienen carta_fecha NULL y se listan como antes.

ALTER TABLE productos ADD COLUMN carta_fecha TEXT;
ALTER TABLE productos ADD COLUMN agotado INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_productos_carta_fecha ON productos(carta_fecha);
