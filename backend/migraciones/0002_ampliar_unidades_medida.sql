-- Amplía las unidades de medida disponibles y, de aquí en adelante, deja
-- de validarlas con un CHECK fijo en la base -- la validación pasa a
-- vivir en el backend (Rust) y en el <select> del frontend.
--
-- SQLite no permite modificar un CHECK existente con ALTER TABLE, hay
-- que reconstruir la tabla completa. Y como ya se confirmó en la práctica
-- (ver bug crítico de reinicio de base de datos en Lubricentro): un
-- RENAME de la tabla productos rompe cualquier trigger que la mencione
-- en su cuerpo, sin importar en qué tabla viva ese trigger. Por eso se
-- sueltan los 4 triggers que referencian productos ANTES del RENAME, y
-- se recrean después, idénticos al schema original.
PRAGMA foreign_keys = OFF;

DROP TRIGGER IF EXISTS trg_sync_stock_lote_insert;
DROP TRIGGER IF EXISTS trg_sync_stock_lote_update;
DROP TRIGGER IF EXISTS trg_after_devolucion_insert;
DROP TRIGGER IF EXISTS trg_after_compra_recibida;

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

CREATE TRIGGER trg_sync_stock_lote_insert
AFTER INSERT ON lotes_producto
BEGIN
  UPDATE productos SET stock = (
    SELECT COALESCE(SUM(cantidad), 0) FROM lotes_producto
    WHERE producto_id = NEW.producto_id AND activo = 1
  )
  WHERE id = NEW.producto_id;
END;

CREATE TRIGGER trg_sync_stock_lote_update
AFTER UPDATE OF cantidad, activo ON lotes_producto
BEGIN
  UPDATE productos SET stock = (
    SELECT COALESCE(SUM(cantidad), 0) FROM lotes_producto
    WHERE producto_id = NEW.producto_id AND activo = 1
  )
  WHERE id = NEW.producto_id;
END;

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

CREATE TRIGGER trg_after_compra_recibida
AFTER UPDATE OF estado ON compras
FOR EACH ROW
WHEN NEW.estado IN ('RECIBIDA', 'PARCIAL') AND OLD.estado = 'PENDIENTE'
BEGIN
  UPDATE productos
  SET stock = stock + (
        SELECT COALESCE(SUM(cantidad_conforme), 0)
        FROM detalles_compra
        WHERE compra_id = NEW.id AND producto_id = productos.id
      ),
      fecha_actualizacion = datetime('now', 'localtime')
  WHERE id IN (
    SELECT producto_id FROM detalles_compra WHERE compra_id = NEW.id
  )
  AND lleva_vencimiento = 0;

  INSERT INTO movimientos_inventario (
    producto_id, tipo_movimiento, cantidad, stock_anterior, stock_nuevo,
    compra_id, usuario_id, referencia, motivo
  )
  SELECT
    dc.producto_id, 'ENTRADA', dc.cantidad_conforme,
    p.stock - dc.cantidad_conforme, p.stock,
    NEW.id, NEW.usuario_id, NEW.folio, 'Recepción de mercadería'
  FROM detalles_compra dc
  JOIN productos p ON p.id = dc.producto_id
  WHERE dc.compra_id = NEW.id AND dc.cantidad_conforme > 0;
END;

PRAGMA foreign_keys = ON;
