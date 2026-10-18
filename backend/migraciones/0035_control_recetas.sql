-- Módulo RECETAS (rubro Farmacia / Botica): constancia de la receta en cada
-- venta de medicamentos que la piden (según la condición de venta de su
-- ficha, migración 0034).
--
-- Solo ADITIVO: dos tablas nuevas. No toca ventas, productos ni stock.
--
--   venta_recetas : una fila por venta que llevó medicamentos con receta.
--     retenida = 1 si llevó alguno de receta retenida (psicotrópicos y
--     estupefacientes): esa venta exige médico, colegiatura, paciente,
--     documento y N.° de receta. En las de "con receta" esos datos son
--     opcionales; la fila deja constancia de que se pidió y se presentó.
--     fecha_hora es hora de Lima. Nunca se borra (si la venta se anula,
--     el libro la muestra anulada).
--
--   venta_receta_productos : qué medicamentos de esa venta pedían receta,
--     con el nombre y la condición que tenían ese día.

CREATE TABLE IF NOT EXISTS venta_recetas (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_id INTEGER NOT NULL UNIQUE,
  retenida INTEGER NOT NULL DEFAULT 0,
  medico_nombre TEXT,
  medico_colegiatura TEXT,
  paciente_nombre TEXT,
  paciente_documento TEXT,
  numero_receta TEXT,
  fecha_receta TEXT,
  usuario_id INTEGER,
  usuario_nombre TEXT,
  fecha_hora TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_venta_recetas_fecha ON venta_recetas (fecha_hora);

CREATE TABLE IF NOT EXISTS venta_receta_productos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  receta_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  producto_nombre TEXT NOT NULL,
  condicion_venta TEXT NOT NULL,
  cantidad REAL NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_receta_productos_receta ON venta_receta_productos (receta_id);
