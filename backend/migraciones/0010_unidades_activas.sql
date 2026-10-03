-- Unidades de venta que usa cada negocio (Configuración → Unidades).
--
-- Solo ADITIVO. La columna guarda los códigos separados por coma
-- ("UNIDAD,PLATO,PORCION"). NULL = el negocio nunca lo configuró: ve las
-- 20 unidades de siempre, igual que antes de esta migración.

ALTER TABLE configuracion_tienda ADD COLUMN unidades_activas TEXT;
