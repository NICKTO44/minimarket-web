-- Trigger: al insertar un detalle de devolución con condición REVENTA,
-- devuelve el stock automáticamente. Si el producto lleva vencimiento,
-- lo devuelve al lote activo más próximo a vencer (mismo criterio FEFO,
-- así el producto devuelto es el primero en salir de nuevo).
DROP TRIGGER IF EXISTS trg_after_devolucion_insert;
CREATE TRIGGER trg_after_devolucion_insert
AFTER INSERT ON detalles_devolucion
FOR EACH ROW
WHEN NEW.condicion = 'REVENTA'
BEGIN
  -- Si el producto lleva vencimiento, sumar al lote más próximo a vencer
  UPDATE lotes_producto
  SET cantidad = cantidad + NEW.cantidad_devuelta
  WHERE id = (
    SELECT id FROM lotes_producto
    WHERE producto_id = NEW.producto_id AND activo = 1
    ORDER BY fecha_vencimiento ASC
    LIMIT 1
  )
  AND (SELECT lleva_vencimiento FROM productos WHERE id = NEW.producto_id) = 1;

  -- Si NO lleva vencimiento, sumar directo al stock del producto
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
