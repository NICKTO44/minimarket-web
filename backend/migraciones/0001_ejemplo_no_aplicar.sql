-- EJEMPLO — bórralo antes de correr el comando por primera vez.
--
-- Convención para migraciones nuevas de aquí en adelante:
--   1. Nombra el archivo con prefijo numérico de 4 dígitos + guión bajo +
--      descripción corta: 0002_agregar_columna_x.sql, 0003_nueva_tabla.sql...
--      El orden numérico es el orden en que se aplican.
--   2. Usa siempre formas seguras de reaplicar (por si alguna vez hay que
--      correr el comando dos veces sobre la misma base):
--        ALTER TABLE productos ADD COLUMN nueva_columna TEXT;
--        CREATE TABLE IF NOT EXISTS ...;
--        CREATE INDEX IF NOT EXISTS ...;
--   3. NUNCA un DROP TABLE + CREATE TABLE dentro de una migración — eso
--      es exactamente el patrón que causó el bug crítico de reinicio de
--      base de datos que ya resolvimos en el proyecto de escritorio.
--   4. Prueba primero contra un negocio de prueba:
--        ./migrar identificador-de-prueba
--      y solo después, contra todos:
--        ./migrar

ALTER TABLE productos ADD COLUMN ejemplo_no_usar TEXT;