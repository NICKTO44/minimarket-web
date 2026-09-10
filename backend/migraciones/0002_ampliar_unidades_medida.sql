-- Amplía las unidades de medida disponibles y, de aquí en adelante, deja
-- de validarlas con un CHECK fijo en la base -- la validación pasa a
-- vivir en el backend (Rust) y en el <select> del frontend.
PRAGMA foreign_keys = OFF;

CREATE TABLE productos_nuevo (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  codigo TEXT NOT NULL UNIQUE,
  nombre TEXT NOT NULL,
  descripcion TEXT,
  precio REAL NOT NULL CHECK (precio > 0),
  stock REAL NOT NULL DEFAULT 0 CHECK (stock >= 0),
  stock_minimo REAL DEFAULT 5,
  unidad_medida TEXT NOT NULL DEFAULT 'UNIDAD',
  categoria_id INTEGER NOT NULL,
  descuento_porcentaje REAL DEFAULT 0,
  lleva_vencimiento INTEGER DEFAULT 0,
  imagen_url TEXT,
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime')),
  precio_compra REAL DEFAULT 0,
  FOREIGN KEY (categoria_id) REFERENCES categorias(id)
);

INSERT INTO productos_nuevo SELECT * FROM productos;

DROP TABLE productos;

ALTER TABLE productos_nuevo RENAME TO productos;

CREATE INDEX idx_productos_codigo ON productos(codigo);
CREATE INDEX idx_productos_nombre ON productos(nombre);
CREATE INDEX idx_productos_categoria ON productos(categoria_id);
CREATE INDEX idx_productos_activo ON productos(activo);

PRAGMA foreign_keys = ON;
