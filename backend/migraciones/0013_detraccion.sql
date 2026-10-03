-- Detracción (SPOT) configurable por negocio (módulo DETRACCION).
--
-- Solo ADITIVO. Todo en NULL = el negocio no la configuró: se usan los
-- valores de la madera (4 %, código 008, mínimo S/ 700) y NO se aplica
-- hasta que el administrador escriba la cuenta del Banco de la Nación.
--   configuracion_tienda.detraccion_porcentaje : % sobre el total con IGV
--   configuracion_tienda.detraccion_codigo     : catálogo 54 de SUNAT ('008' madera)
--   configuracion_tienda.detraccion_minimo     : se aplica si el total SUPERA este monto
--   configuracion_tienda.detraccion_cuenta     : cuenta de detracciones (Banco de la Nación)
-- En el comprobante queda lo que se envió, para reimprimirlo igual aunque
-- después cambie el porcentaje.

ALTER TABLE configuracion_tienda ADD COLUMN detraccion_porcentaje REAL;
ALTER TABLE configuracion_tienda ADD COLUMN detraccion_codigo TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN detraccion_minimo REAL;
ALTER TABLE configuracion_tienda ADD COLUMN detraccion_cuenta TEXT;
ALTER TABLE comprobantes_electronicos ADD COLUMN detraccion_porcentaje REAL;
ALTER TABLE comprobantes_electronicos ADD COLUMN detraccion_monto REAL;
ALTER TABLE comprobantes_electronicos ADD COLUMN detraccion_cuenta TEXT;
