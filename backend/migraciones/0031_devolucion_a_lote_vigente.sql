-- Devolución en REVENTA de un producto con vencimiento: lo devuelto vuelve
-- al lote VIGENTE que vence primero, no al que vence primero a secas (que
-- podía ser uno ya vencido, y entonces lo devuelto quedaba sin poder
-- venderse). Si el producto no tiene ningún lote vigente, vuelve al lote
-- activo que vence más tarde, como último recurso, para no perder stock.
--
-- Vigente = vence hoy o después, hora de Lima (UTC-5, sin horario de
-- verano), igual que la venta (ventas.rs, descontar_stock_fefo).
--
-- Solo cambia el ORDER BY del primer UPDATE; el resto del trigger es
-- idéntico al de schema.sql. No toca tablas.

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
    ORDER BY
      CASE WHEN date(fecha_vencimiento) >= date('now', '-5 hours') THEN 0 ELSE 1 END ASC,
      CASE WHEN date(fecha_vencimiento) >= date('now', '-5 hours') THEN fecha_vencimiento END ASC,
      fecha_vencimiento DESC
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
