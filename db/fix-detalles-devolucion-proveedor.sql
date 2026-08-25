-- Corrige detalles_devolucion_proveedor.cantidad_devuelta de INTEGER a REAL,
-- mismo problema que ya se había corregido en detalles_devolucion (cliente).
-- Preserva los datos existentes.

CREATE TABLE detalles_devolucion_proveedor_nueva (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  devolucion_proveedor_id INTEGER NOT NULL,
  detalle_compra_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  cantidad_devuelta REAL NOT NULL CHECK (cantidad_devuelta > 0),
  precio_compra REAL NOT NULL,
  subtotal REAL NOT NULL,
  motivo_item TEXT,
  FOREIGN KEY (devolucion_proveedor_id) REFERENCES devoluciones_proveedor(id) ON DELETE CASCADE,
  FOREIGN KEY (detalle_compra_id) REFERENCES detalles_compra(id),
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);

INSERT INTO detalles_devolucion_proveedor_nueva
SELECT id, devolucion_proveedor_id, detalle_compra_id, producto_id,
       CAST(cantidad_devuelta AS REAL), precio_compra, subtotal, motivo_item
FROM detalles_devolucion_proveedor;

DROP TABLE detalles_devolucion_proveedor;
ALTER TABLE detalles_devolucion_proveedor_nueva RENAME TO detalles_devolucion_proveedor;

CREATE INDEX idx_detdevprov_devolucion ON detalles_devolucion_proveedor(devolucion_proveedor_id);
CREATE INDEX idx_detdevprov_producto ON detalles_devolucion_proveedor(producto_id);
