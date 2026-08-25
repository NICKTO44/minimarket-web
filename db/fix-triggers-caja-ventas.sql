-- Triggers de caja que faltaban: sincronizan automáticamente los
-- acumulados de la caja abierta cuando se registra una venta, una
-- devolución, o se cancela una venta. Sin esto, la caja nunca "se entera"
-- de las ventas y el cuadre siempre cierra mal.

DROP TRIGGER IF EXISTS trg_actualizar_caja_venta;
CREATE TRIGGER trg_actualizar_caja_venta
AFTER INSERT ON ventas
FOR EACH ROW
WHEN NEW.estado = 'COMPLETADA'
BEGIN
  UPDATE cajas
  SET
    ventas_efectivo      = ventas_efectivo      + CASE WHEN NEW.metodo_pago = 'EFECTIVO'      THEN NEW.total ELSE 0 END,
    ventas_tarjeta       = ventas_tarjeta       + CASE WHEN NEW.metodo_pago = 'TARJETA'       THEN NEW.total ELSE 0 END,
    ventas_transferencia = ventas_transferencia + CASE WHEN NEW.metodo_pago IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN NEW.total ELSE 0 END,
    total_ventas         = total_ventas         + NEW.total,
    numero_transacciones = numero_transacciones + 1,
    cambio_total         = cambio_total         + COALESCE(NEW.cambio, 0),
    ticket_promedio      = (total_ventas + NEW.total) / (numero_transacciones + 1)
  WHERE estado = 'ABIERTA';
END;

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

DROP TRIGGER IF EXISTS trg_actualizar_caja_cancelar_venta;
CREATE TRIGGER trg_actualizar_caja_cancelar_venta
AFTER UPDATE ON ventas
FOR EACH ROW
WHEN OLD.estado = 'COMPLETADA' AND NEW.estado = 'CANCELADA'
BEGIN
  UPDATE cajas
  SET
    ventas_efectivo      = ventas_efectivo      - CASE WHEN NEW.metodo_pago = 'EFECTIVO'      THEN NEW.total ELSE 0 END,
    ventas_tarjeta       = ventas_tarjeta       - CASE WHEN NEW.metodo_pago = 'TARJETA'       THEN NEW.total ELSE 0 END,
    ventas_transferencia = ventas_transferencia - CASE WHEN NEW.metodo_pago IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN NEW.total ELSE 0 END,
    total_ventas         = total_ventas         - NEW.total,
    numero_transacciones = numero_transacciones - 1
  WHERE estado = 'ABIERTA';
END;
