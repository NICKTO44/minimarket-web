-- La tabla devoluciones se perdió en un intento de migración anterior.
-- La recreamos desde cero con el CHECK ya corregido (incluye TRANSFERENCIA).
-- No hay datos que preservar: solo existía 1 registro de prueba.

DROP TABLE IF EXISTS devoluciones;

CREATE TABLE devoluciones (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_original_id INTEGER NOT NULL,
  folio_devolucion TEXT NOT NULL UNIQUE,
  fecha_hora TEXT DEFAULT (datetime('now', 'localtime')),
  usuario_id INTEGER NOT NULL,
  monto_reembolsado REAL NOT NULL CHECK (monto_reembolsado >= 0),
  metodo_reembolso TEXT NOT NULL CHECK(metodo_reembolso IN ('EFECTIVO', 'TARJETA', 'TRANSFERENCIA', 'VALE', 'CREDITO')),
  motivo TEXT NOT NULL,
  estado TEXT DEFAULT 'PROCESADA' CHECK(estado IN ('PROCESADA', 'PENDIENTE', 'RECHAZADA')),
  notas TEXT,
  FOREIGN KEY (venta_original_id) REFERENCES ventas(id),
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id)
);

CREATE INDEX idx_devoluciones_venta ON devoluciones(venta_original_id);
CREATE INDEX idx_devoluciones_folio ON devoluciones(folio_devolucion);
CREATE INDEX idx_devoluciones_fecha ON devoluciones(fecha_hora);

DROP TRIGGER IF EXISTS trg_actualizar_caja_devolucion;
CREATE TRIGGER trg_actualizar_caja_devolucion
AFTER INSERT ON devoluciones
FOR EACH ROW
WHEN NEW.estado = 'PROCESADA'
BEGIN
  UPDATE cajas
  SET
    devoluciones_monto    = devoluciones_monto + NEW.monto_reembolsado,
    devoluciones_cantidad = devoluciones_cantidad + 1,
    ventas_efectivo       = ventas_efectivo       - CASE WHEN NEW.metodo_reembolso = 'EFECTIVO'      THEN NEW.monto_reembolsado ELSE 0 END,
    ventas_tarjeta        = ventas_tarjeta        - CASE WHEN NEW.metodo_reembolso = 'TARJETA'       THEN NEW.monto_reembolsado ELSE 0 END,
    ventas_transferencia  = ventas_transferencia  - CASE WHEN NEW.metodo_reembolso = 'TRANSFERENCIA' THEN NEW.monto_reembolsado ELSE 0 END
  WHERE estado = 'ABIERTA';
END;

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
