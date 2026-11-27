-- Módulo "Impresión de comandas" (COMANDAS, restaurante): la comanda sale
-- sola en la ticketera de cocina desde la estación de comandas (la pantalla
-- Comandas abierta en la computadora de esa ticketera), y no en el celular
-- del mozo que envió el pedido. La cocina no marca "listo".
--
-- pedido_items.comanda_impresa: cuándo la estación imprimió esa línea. Se
-- toma de forma atómica (UPDATE ... RETURNING): nunca sale dos veces. Con el
-- módulo apagado, lo enviado se da por impreso al enviarse (salió en el
-- equipo del que lo mandó), así al encender el módulo no salen comandas viejas.
--
-- Solo ADITIVO: una columna nueva. Lo ya enviado se da por impreso.
ALTER TABLE pedido_items ADD COLUMN comanda_impresa TEXT;
UPDATE pedido_items SET comanda_impresa = COALESCE(fecha_envio, datetime('now', 'localtime')) WHERE estado != 'PENDIENTE';
