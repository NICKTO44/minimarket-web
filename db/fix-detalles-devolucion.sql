-- Corrige detalles_devolucion.cantidad_devuelta de INTEGER a REAL,
-- consistente con el resto de columnas de cantidad en el sistema
-- (productos.stock, detalles_venta.cantidad, lotes_producto.cantidad
-- son todas REAL). Preserva los datos existentes.

CREATE TABLE detalles_devolucion_nueva (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  devolucion_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  detalle_venta_id INTEGER,
  venta_id INTEGER NOT NULL,
  cantidad_devuelta REAL NOT NULL CHECK (cantidad_devuelta > 0),
  precio_unitario REAL NOT NULL,
  subtotal REAL NOT NULL CHECK (subtotal >= 0),
  condicion TEXT NOT NULL CHECK(condicion IN ('REVENTA', 'DEFECTUOSO', 'VENCIDO')),
  FOREIGN KEY (devolucion_id) REFERENCES devoluciones(id) ON DELETE CASCADE,
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);

INSERT INTO detalles_devolucion_nueva
SELECT id, devolucion_id, producto_id, detalle_venta_id, venta_id,
       CAST(cantidad_devuelta AS REAL), precio_unitario, subtotal, condicion
FROM detalles_devolucion;

DROP TABLE detalles_devolucion;
ALTER TABLE detalles_devolucion_nueva RENAME TO detalles_devolucion;

CREATE INDEX idx_detalles_devolucion ON detalles_devolucion(devolucion_id);
CREATE INDEX idx_detalles_devolucion_producto ON detalles_devolucion(producto_id);

-- Recrear el trigger (se elimina automáticamente al hacer DROP TABLE)
DROP TRIGGER IF EXISTS trg_after_devolucion_insert;
CREATE TRIGGER trg_after_devolucion_insert
AFTER INSERT ON detalles_devolucion
FOR EACH ROW
WHEN NEW.condicion = 'REVENTA'
BEGIN
  UPDATE lotes_producto
  SET cantidad = cantidad + NEW.cantidad_devuelta
  WHERE id = (
    SELECT id FROM lotes_producto
    WHERE producto_id = NEW.producto_id AND activo = 1
    ORDER BY fecha_vencimiento ASC
    LIMIT 1
  )
  AND (SELECT lleva_vencimiento FROM productos WHERE id = NEW.producto_id) = 1;

  UPDATE productos
  SET stock = stock + NEW.cantidad_devuelta,
      fecha_actualizacion = datetime('now', 'localtime')
  WHERE id = NEW.producto_id
    AND (SELECT lleva_vencimiento FROM productos WHERE id = NEW.producto_id) = 0;

  INSERT INTO movimientos_inventario (
    producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo,
    devolucion_id, usuario_id, motivo
  ) VALUES (
    NEW.producto_id, 'DEVOLUCION', NEW.cantidad_devuelta,
    (SELECT stock - NEW.cantidad_devuelta FROM productos WHERE id = NEW.producto_id),
    (SELECT stock FROM productos WHERE id = NEW.producto_id),
    NEW.devolucion_id,
    (SELECT usuario_id FROM devoluciones WHERE id = NEW.devolucion_id),
    'Devolución - Condición: REVENTA'
  );
END;
