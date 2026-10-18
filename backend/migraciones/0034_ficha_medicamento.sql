-- Módulo FICHA_MEDICAMENTO (rubro Farmacia / Botica): los datos de un
-- producto que es medicamento.
--
-- Solo ADITIVO: una tabla nueva. No toca productos, ventas ni stock. Un
-- producto sin fila aquí es un producto común (en una botica también se
-- venden pañales o golosinas).
--
--   producto_medicamento : una fila por producto que es medicamento.
--     principio_activo   : DCI, como se escribió ("Paracetamol + Cafeína").
--     principio_clave    : el mismo en minúsculas, sin tildes y con los
--                          componentes ordenados ("cafeina + paracetamol"),
--                          para buscar y para encontrar equivalentes.
--     concentracion      : "500 mg", "250 mg/5 ml" (concentracion_clave: sin
--                          espacios y en minúsculas, "250mg/5ml").
--     forma_farmaceutica : código de la lista de formas (TABLETA, JARABE...).
--     laboratorio, registro_sanitario : como figuran en el envase.
--     condicion_venta    : SIN_RECETA, CON_RECETA o RECETA_RETENIDA
--                          (psicotrópicos y estupefacientes). El módulo de
--                          recetas usa este dato para pedir la receta.
--     es_generico        : 1 si es genérico (se vende por su DCI).

CREATE TABLE IF NOT EXISTS producto_medicamento (
  producto_id INTEGER PRIMARY KEY,
  principio_activo TEXT NOT NULL,
  principio_clave TEXT NOT NULL,
  concentracion TEXT,
  concentracion_clave TEXT,
  forma_farmaceutica TEXT,
  laboratorio TEXT,
  registro_sanitario TEXT,
  condicion_venta TEXT NOT NULL DEFAULT 'SIN_RECETA'
    CHECK (condicion_venta IN ('SIN_RECETA', 'CON_RECETA', 'RECETA_RETENIDA')),
  es_generico INTEGER NOT NULL DEFAULT 0,
  fecha_actualizacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_medicamento_principio ON producto_medicamento (principio_clave, concentracion_clave, forma_farmaceutica);
