-- Rol VENDEDOR: lo mismo que el administrador del negocio, MENOS
-- Configuración (datos del negocio, usuarios, rubro y módulos, unidades,
-- mesas...) y Suscripción. Ver middleware_auth.rs (exigir_admin deja pasar
-- al vendedor; exigir_solo_admin, no).
--
-- Solo ADITIVO: un rol nuevo. No cambia a ningún usuario ni a otro rol. Se
-- ofrece en todos los negocios al crear usuarios.
INSERT INTO roles (nombre, descripcion)
SELECT 'VENDEDOR', 'Todo menos Configuración y Suscripción'
WHERE NOT EXISTS (SELECT 1 FROM roles WHERE nombre = 'VENDEDOR');
