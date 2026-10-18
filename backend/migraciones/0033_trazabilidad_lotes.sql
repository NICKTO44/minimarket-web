-- Trazabilidad de lotes (base para farmacia) y referencia a la nota de
-- crédito del proveedor.
--
-- Solo ADITIVO: una tabla, una columna y el trigger de devoluciones
-- rehecho. No reconstruye tablas.
--
--   venta_lotes : de qué lote salió cada unidad vendida de un producto con
--     vencimiento. La llena la venta (ventas.rs, descontar_stock_fefo), en
--     la misma transacción. Con ella se sabe a quién se vendió un lote (por
--     ejemplo, ante una alerta de retiro) y la devolución del cliente vuelve
--     a su lote de origen.
--
--   devoluciones_proveedor.nota_credito_proveedor : número de la nota de
--     crédito electrónica que EMITIÓ EL PROVEEDOR por esa devolución (p. ej.
--     "FC01-00000123"). Es solo una referencia para cruzar con el contador;
--     Monspeet no la envía ni la consulta en SUNAT.

CREATE TABLE IF NOT EXISTS venta_lotes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  lote_id INTEGER NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0)
);

CREATE INDEX IF NOT EXISTS idx_venta_lotes_venta ON venta_lotes (venta_id, producto_id);
CREATE INDEX IF NOT EXISTS idx_venta_lotes_lote ON venta_lotes (lote_id);

ALTER TABLE devoluciones_proveedor ADD COLUMN nota_credito_proveedor TEXT;

-- Devolución en REVENTA de un producto con vencimiento, en este orden:
--   1. al lote de donde salió en esa venta (venta_lotes), si sigue vigente;
--   2. si no, al lote vigente que vence primero (como en la 0031);
--   3. si no hay ninguno vigente, al que venció más tarde.
-- Vigente = vence hoy o después, hora de Lima. El resto del trigger es
-- idéntico al de la 0031.

DROP TRIGGER IF EXISTS trg_after_devolucion_insert;

CREATE TRIGGER trg_after_devolucion_insert
AFTER INSERT ON detalles_devolucion
FOR EACH ROW
WHEN NEW.condicion = 'REVENTA'
BEGIN
  UPDATE lotes_producto
  SET cantidad = cantidad + NEW.cantidad_devuelta
  WHERE id = (
    SELECT l.id FROM lotes_producto l
    WHERE l.producto_id = NEW.producto_id AND l.activo = 1
    ORDER BY
      CASE
        WHEN date(l.fecha_vencimiento) >= date('now', '-5 hours')
         AND l.id IN (SELECT vl.lote_id FROM venta_lotes vl
                      WHERE vl.venta_id = NEW.venta_id AND vl.producto_id = NEW.producto_id) THEN 0
        WHEN date(l.fecha_vencimiento) >= date('now', '-5 hours') THEN 1
        ELSE 2
      END ASC,
      CASE WHEN date(l.fecha_vencimiento) >= date('now', '-5 hours') THEN l.fecha_vencimiento END ASC,
      l.fecha_vencimiento DESC
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
