-- Módulo COSTEO ("Margen por plato"), solo rubro Restaurante. Ver
-- handlers/costeo.rs.
--
-- Solo ADITIVO: tablas nuevas. No toca productos, ventas ni el reporte de
-- ganancias. Los insumos del costeo NO son productos: no aparecen en el
-- punto de venta ni mueven stock; solo sirven para saber cuánto cuesta
-- cada plato.
--
--   costeo_config : una sola fila (id = 1).
--     regimen             : GENERAL (cobra IGV), RUS (Nuevo RUS, sin IGV) o
--                           NO_SE (se calcula como GENERAL y se avisa).
--     compras_con_factura : 1 = compra sus insumos con factura y recupera el
--                           IGV (el costo se toma sin IGV).
--     costo_porcentaje    : costo aproximado de un plato sin receta ni costo,
--                           en % de su precio sin IGV (food cost).
--     margen_objetivo     : margen mínimo que quiere ganar en cada plato, en %
--                           de su precio sin IGV. Debajo de eso, alerta.
--     gastos_mensuales    : gastos fijos aproximados del mes, si no los
--                           registra en el módulo Gastos.
--   costeo_categorias : por categoría, el % de costo aproximado y el margen
--                       objetivo (las bebidas que se compran y revenden ganan
--                       menos que un plato). NULL = usa el general.
--   costeo_insumos    : ingredientes con su precio de compra por KG, LITRO o
--                       UNIDAD, tal como lo paga (con IGV si lo trae).
--     con_igv : la factura de este insumo trae IGV (las verduras o frutas
--               frescas suelen venir exoneradas).
--   costeo_platos     : costo de un plato que no tiene receta (costo_fijo) y
--                       cuántas porciones rinde su receta.
--   costeo_receta     : lo que lleva cada plato; cantidad en gramos (insumo
--                       por KG), mililitros (por LITRO) o unidades.

CREATE TABLE IF NOT EXISTS costeo_config (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  regimen TEXT NOT NULL DEFAULT 'NO_SE' CHECK (regimen IN ('GENERAL', 'RUS', 'NO_SE')),
  compras_con_factura INTEGER NOT NULL DEFAULT 0,
  costo_porcentaje REAL NOT NULL DEFAULT 35 CHECK (costo_porcentaje > 0 AND costo_porcentaje < 100),
  margen_objetivo REAL NOT NULL DEFAULT 65 CHECK (margen_objetivo >= 0 AND margen_objetivo < 100),
  gastos_mensuales REAL NOT NULL DEFAULT 0 CHECK (gastos_mensuales >= 0),
  fecha_actualizacion TEXT
);

INSERT OR IGNORE INTO costeo_config (id) VALUES (1);

CREATE TABLE IF NOT EXISTS costeo_categorias (
  categoria_id INTEGER PRIMARY KEY,
  costo_porcentaje REAL CHECK (costo_porcentaje IS NULL OR (costo_porcentaje > 0 AND costo_porcentaje < 100)),
  margen_objetivo REAL CHECK (margen_objetivo IS NULL OR (margen_objetivo >= 0 AND margen_objetivo < 100))
);

CREATE TABLE IF NOT EXISTS costeo_insumos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL,
  unidad TEXT NOT NULL DEFAULT 'KG' CHECK (unidad IN ('KG', 'LITRO', 'UNIDAD')),
  precio REAL NOT NULL CHECK (precio >= 0),
  con_igv INTEGER NOT NULL DEFAULT 1,
  activo INTEGER NOT NULL DEFAULT 1,
  fecha_actualizacion TEXT
);

CREATE TABLE IF NOT EXISTS costeo_platos (
  producto_id INTEGER PRIMARY KEY,
  costo_fijo REAL CHECK (costo_fijo IS NULL OR costo_fijo >= 0),
  rinde REAL NOT NULL DEFAULT 1 CHECK (rinde > 0),
  nota TEXT,
  fecha_actualizacion TEXT
);

CREATE TABLE IF NOT EXISTS costeo_receta (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  insumo_id INTEGER NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0)
);

CREATE INDEX IF NOT EXISTS idx_costeo_receta_producto ON costeo_receta (producto_id);
CREATE INDEX IF NOT EXISTS idx_costeo_receta_insumo ON costeo_receta (insumo_id);
