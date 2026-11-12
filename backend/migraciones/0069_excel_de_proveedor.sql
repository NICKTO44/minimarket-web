-- Compras por Excel: lo que el sistema aprende del Excel de cada proveedor.
-- proveedor_productos: su código (C:...) o el nombre que usa (N:...) -> nuestro producto.
-- proveedor_excel: qué columna de su Excel es el código, el producto, la cantidad, etc. (JSON).
CREATE TABLE IF NOT EXISTS proveedor_productos (
  proveedor_id INTEGER NOT NULL,
  clave TEXT NOT NULL,
  producto_id INTEGER NOT NULL,
  fecha_actualizacion TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (proveedor_id, clave)
);

CREATE TABLE IF NOT EXISTS proveedor_excel (
  proveedor_id INTEGER PRIMARY KEY,
  columnas TEXT NOT NULL,
  fecha_actualizacion TEXT NOT NULL DEFAULT (datetime('now'))
);
