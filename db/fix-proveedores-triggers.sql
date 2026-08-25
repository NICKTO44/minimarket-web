-- Agrega la columna precio_compra a productos (existía en Lubricentro
-- vía migración idempotente en el código Rust, faltaba en el schema base)
ALTER TABLE productos ADD COLUMN precio_compra REAL DEFAULT 0;

-- =====================================================
-- TRIGGER: al recibir mercadería (PENDIENTE -> RECIBIDA/PARCIAL),
-- sube el stock del producto por lo que llegó conforme, y registra
-- el movimiento de inventario. Adaptado sin variantes/tallas.
-- Si el producto lleva vencimiento, el stock real lo gestiona el
-- endpoint creando un lote nuevo (no este trigger) — este trigger
-- solo actualiza productos.stock para productos SIN vencimiento.
-- =====================================================
DROP TRIGGER IF EXISTS trg_after_compra_recibida;
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

-- =====================================================
-- TRIGGER: recalcula subtotal/total/saldo/estado_pago de la compra
-- según lo que realmente llegó conforme (puede diferir de lo pedido).
-- =====================================================
DROP TRIGGER IF EXISTS trg_recalcular_total_compra;
CREATE TRIGGER trg_recalcular_total_compra
AFTER UPDATE OF estado ON compras
FOR EACH ROW
WHEN NEW.estado IN ('RECIBIDA', 'PARCIAL') AND OLD.estado = 'PENDIENTE'
BEGIN
  UPDATE compras
  SET
    subtotal = (
      SELECT COALESCE(SUM(cantidad_conforme * precio_compra), 0)
      FROM detalles_compra WHERE compra_id = NEW.id
    ),
    total = (
      SELECT COALESCE(SUM(cantidad_conforme * precio_compra), 0)
      FROM detalles_compra WHERE compra_id = NEW.id
    ) - COALESCE(NEW.descuento, 0) - COALESCE(NEW.credito_aplicado, 0),
    saldo_pendiente = CASE
      WHEN (
        SELECT COALESCE(SUM(cantidad_conforme * precio_compra), 0)
        FROM detalles_compra WHERE compra_id = NEW.id
      ) - COALESCE(NEW.descuento, 0) - COALESCE(NEW.credito_aplicado, 0) - NEW.monto_pagado < 0
      THEN 0
      ELSE (
        SELECT COALESCE(SUM(cantidad_conforme * precio_compra), 0)
        FROM detalles_compra WHERE compra_id = NEW.id
      ) - COALESCE(NEW.descuento, 0) - COALESCE(NEW.credito_aplicado, 0) - NEW.monto_pagado
    END,
    estado_pago = CASE
      WHEN NEW.monto_pagado >= (
        SELECT COALESCE(SUM(cantidad_conforme * precio_compra), 0)
        FROM detalles_compra WHERE compra_id = NEW.id
      ) - COALESCE(NEW.descuento, 0) - COALESCE(NEW.credito_aplicado, 0)
      THEN 'PAGADO'
      WHEN NEW.monto_pagado > 0 THEN 'PARCIAL'
      ELSE 'PENDIENTE'
    END,
    fecha_actualizacion = datetime('now', 'localtime')
  WHERE id = NEW.id;
END;

-- =====================================================
-- TRIGGER: al crear una compra usando crédito del proveedor,
-- descuenta automáticamente ese crédito disponible.
-- =====================================================
DROP TRIGGER IF EXISTS trg_descontar_credito_proveedor;
CREATE TRIGGER trg_descontar_credito_proveedor
AFTER INSERT ON compras
FOR EACH ROW
WHEN NEW.credito_aplicado > 0
BEGIN
  UPDATE proveedores
  SET credito_disponible = credito_disponible - NEW.credito_aplicado,
      fecha_actualizacion = datetime('now', 'localtime')
  WHERE id = NEW.proveedor_id;
END;
