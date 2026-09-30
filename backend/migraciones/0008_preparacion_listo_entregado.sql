-- Módulo Cafetería / Restaurante: aviso "pedido listo".
--
-- Solo ADITIVO (se aplica solo a todos los negocios, existentes y nuevos;
-- en los que están en modo Tienda no cambia nada).
--
-- Etapas de una línea enviada a preparar (estado = 'ENVIADO'):
--   fecha_listo NULL                      -> en preparación
--   fecha_listo con valor                 -> LISTO (lo marcó barra/cocina)
--   fecha_entregado con valor             -> ENTREGADO (lo marcó el mozo/cajero)
-- Se usan columnas nuevas en vez de ampliar el CHECK de "estado", porque
-- cambiar un CHECK en SQLite obliga a reconstruir la tabla.

ALTER TABLE pedido_items ADD COLUMN fecha_listo TEXT;
ALTER TABLE pedido_items ADD COLUMN fecha_entregado TEXT;
ALTER TABLE pedido_items ADD COLUMN listo_por INTEGER;

CREATE INDEX IF NOT EXISTS idx_pedido_items_preparacion
  ON pedido_items(estado, fecha_entregado, fecha_listo);

-- Rol de barra/cocina: solo ve la pantalla Preparación y marca "Listo".
INSERT INTO roles (nombre, descripcion)
SELECT 'PREPARACION', 'Barra o cocina: marca los pedidos listos; no cobra ni ve mesas'
WHERE NOT EXISTS (SELECT 1 FROM roles WHERE nombre = 'PREPARACION');
