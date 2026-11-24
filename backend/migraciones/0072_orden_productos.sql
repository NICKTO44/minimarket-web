-- Orden de la lista de productos (Configuración → casilla "Ordenar productos
-- por código interno"). Pedido de PFASA: ver sus productos y sus Excel por
-- código. NOMBRE (por defecto) = como siempre; CODIGO = por código interno,
-- de menor a mayor y por número (9001 antes que 19001).
--
-- Solo ADITIVO: una columna con valor por defecto. Ningún negocio cambia
-- hasta que su administrador marque la casilla.
ALTER TABLE configuracion_tienda ADD COLUMN orden_productos TEXT NOT NULL DEFAULT 'NOMBRE';
