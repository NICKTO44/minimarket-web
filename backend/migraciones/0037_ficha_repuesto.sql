-- Módulo FICHA_REPUESTO (rubro Lubricentro / Repuestos): los datos de un
-- producto que es repuesto, lubricante o accesorio de vehículo.
--
-- Solo ADITIVO: dos tablas nuevas. No toca productos, ventas ni stock. Un
-- producto sin fila aquí es un producto común (en un lubricentro también
-- se venden ambientadores o franelas).
--
--   producto_repuesto : una fila por producto con ficha.
--     marca              : fabricante del repuesto ("Bosch", "Castrol").
--     numero_parte       : código del fabricante u OEM ("90915-YZZE1").
--     numero_parte_clave : el mismo sin espacios, guiones ni puntos y en
--                          mayúsculas ("90915YZZE1"), para buscar.
--     ubicacion          : dónde está en el almacén ("Estante 3 - B").
--     garantia_dias      : días de garantía que salen en el ticket (NULL o
--                          0 = sin garantía).
--
--   producto_codigos_alternos : otros códigos con los que se pide el mismo
--     repuesto (el de Toyota, el de Wix, el de Bosch...). Dos productos que
--     comparten un código (o el n.° de parte de uno es código alterno del
--     otro) son EQUIVALENTES: uno reemplaza al otro.

CREATE TABLE IF NOT EXISTS producto_repuesto (
  producto_id INTEGER PRIMARY KEY,
  marca TEXT,
  numero_parte TEXT,
  numero_parte_clave TEXT,
  ubicacion TEXT,
  garantia_dias INTEGER,
  fecha_actualizacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_repuesto_parte ON producto_repuesto (numero_parte_clave);

CREATE TABLE IF NOT EXISTS producto_codigos_alternos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  codigo TEXT NOT NULL,
  codigo_clave TEXT NOT NULL,
  fabricante TEXT,
  UNIQUE (producto_id, codigo_clave)
);

CREATE INDEX IF NOT EXISTS idx_codigos_alternos_clave ON producto_codigos_alternos (codigo_clave);
