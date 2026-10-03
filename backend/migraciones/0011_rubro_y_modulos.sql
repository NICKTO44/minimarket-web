-- Rubro del negocio y módulos encendidos (Configuración → Rubro y módulos).
--
-- Solo ADITIVO. Ambas columnas en NULL = negocio que nunca lo configuró:
-- se comporta igual que antes de esta migración (el que atiende en mesas se
-- ve como rubro RESTAURANTE; los demás, como rubro GENERAL sin módulos extra).
--   rubro   : BODEGA, RESTAURANTE, FERRETERIA, MADERERA o GENERAL.
--   modulos : módulos extra encendidos, separados por coma (hoy: SERVICIOS).
--             El módulo MESAS no se guarda aquí: sigue siendo
--             modo_negocio = 'RESTAURANTE', como desde la migración 0007.

ALTER TABLE configuracion_tienda ADD COLUMN rubro TEXT;
ALTER TABLE configuracion_tienda ADD COLUMN modulos TEXT;
