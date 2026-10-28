-- Dark kitchen (módulo DELIVERY): el repartidor con su propia cuenta.
--
-- Solo ADITIVO: un rol nuevo y columnas nuevas en pedido_delivery (0045).
--  - El cajero asigna cada pedido a un repartidor (usuario con rol
--    REPARTIDOR) con su orden de entrega del día.
--  - En contra entrega, el repartidor cobra delante del cliente desde su
--    cuenta (sale la nota de venta o boleta y se le envía por WhatsApp). El
--    efectivo que cobró queda "por rendir" hasta que el cajero marca que lo
--    recibió (rendido_en).
-- El rol solo se ofrece al crear usuarios si el módulo está encendido.
INSERT INTO roles (nombre, descripcion)
SELECT 'REPARTIDOR', 'Lleva los pedidos delivery asignados; cobra contra entrega'
WHERE NOT EXISTS (SELECT 1 FROM roles WHERE nombre = 'REPARTIDOR');

ALTER TABLE pedido_delivery ADD COLUMN repartidor_id INTEGER;
ALTER TABLE pedido_delivery ADD COLUMN orden_reparto INTEGER;
ALTER TABLE pedido_delivery ADD COLUMN rendido_en TEXT;
ALTER TABLE pedido_delivery ADD COLUMN rendido_por INTEGER;

CREATE INDEX IF NOT EXISTS idx_pedido_delivery_repartidor ON pedido_delivery(repartidor_id, estado);
