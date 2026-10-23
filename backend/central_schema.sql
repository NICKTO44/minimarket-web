-- Estructura de la base central de Monspeet (minimarket-central).
-- Sacada de producción el 22-oct-2026 con `turso db shell minimarket-central .schema`.
-- Solo estructura, sin datos.
--
-- Sirve para crear la central de un entorno nuevo (desarrollo):
--   turso db shell <central-de-desarrollo> < central_schema.sql
-- Usa IF NOT EXISTS: correrlo dos veces no da error ni borra nada.
--
-- La columna `slug` también la agrega el backend al arrancar
-- (handlers/direcciones.rs, preparar_central) si falta.

CREATE TABLE IF NOT EXISTS codigos_activacion (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    codigo TEXT NOT NULL UNIQUE,
    duracion_cantidad INTEGER NOT NULL,
    duracion_unidad TEXT NOT NULL CHECK (duracion_unidad IN ('DIA', 'MES', 'ANIO')),
    usado INTEGER NOT NULL DEFAULT 0,
    usado_por_tienda_id INTEGER,
    fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
    fecha_uso TEXT,
    fecha_expira_si_no_se_usa TEXT
);
CREATE INDEX IF NOT EXISTS idx_codigos_usado ON codigos_activacion(usado);

CREATE TABLE IF NOT EXISTS tiendas (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre_negocio TEXT NOT NULL,
    identificador TEXT NOT NULL UNIQUE,
    ruc TEXT,
    turso_db_url TEXT NOT NULL,
    turso_db_token TEXT NOT NULL,
    fecha_creacion TEXT NOT NULL DEFAULT (datetime('now')),
    estado TEXT NOT NULL DEFAULT 'ACTIVO' CHECK (estado IN ('ACTIVO', 'RESTRINGIDO', 'SUSPENDIDO')),
    fecha_vencimiento TEXT,
    slug TEXT,
    -- CLIENTE (negocio real) o DEMO (cuenta de prueba). La agrega el backend
    -- al arrancar si falta (handlers/panel.rs, preparar_tipos).
    tipo TEXT NOT NULL DEFAULT 'CLIENTE' CHECK (tipo IN ('CLIENTE', 'DEMO'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tiendas_slug ON tiendas (slug);

CREATE TABLE IF NOT EXISTS usuarios_indice (
    usuario TEXT PRIMARY KEY,
    tienda_id INTEGER NOT NULL REFERENCES tiendas(id)
);
