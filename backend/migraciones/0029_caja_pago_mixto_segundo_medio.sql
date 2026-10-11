-- Triggers de caja con el pago mixto entre dos medios (migración 0028).
--
-- Iguales a los de la 0004, con un cambio: la parte guardada en
-- `pago_efectivo` va a la línea de su medio (`pago_efectivo_metodo`), no
-- siempre a efectivo. Con NULL (todo lo anterior) se suma en efectivo,
-- exactamente como antes. Yape y Plin siguen sumando en transferencia; su
-- detalle por billetera lo lleva el servidor (migración 0019).
--
-- El trigger de devoluciones no cambia: va por `metodo_reembolso`.
-- Se puede volver a correr entero (DROP IF EXISTS + CREATE).

DROP TRIGGER IF EXISTS trg_actualizar_caja_venta;
DROP TRIGGER IF EXISTS trg_actualizar_caja_cancelar_venta;

CREATE TRIGGER trg_actualizar_caja_venta
AFTER INSERT ON ventas
FOR EACH ROW
WHEN NEW.estado = 'COMPLETADA'
BEGIN
  UPDATE cajas
  SET
    ventas_efectivo      = ventas_efectivo + CASE
                             WHEN NEW.metodo_pago = 'EFECTIVO' THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' AND COALESCE(NEW.pago_efectivo_metodo, 'EFECTIVO') = 'EFECTIVO'
                               THEN COALESCE(NEW.pago_efectivo, 0)
                             ELSE 0 END,
    ventas_tarjeta       = ventas_tarjeta + CASE
                             WHEN NEW.metodo_pago = 'TARJETA' THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' THEN
                                 CASE WHEN NEW.pago_otro_metodo = 'TARJETA' THEN COALESCE(NEW.pago_otro, 0) ELSE 0 END
                               + CASE WHEN NEW.pago_efectivo_metodo = 'TARJETA' THEN COALESCE(NEW.pago_efectivo, 0) ELSE 0 END
                             ELSE 0 END,
    ventas_transferencia = ventas_transferencia + CASE
                             WHEN NEW.metodo_pago IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' THEN
                                 CASE WHEN NEW.pago_otro_metodo IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN COALESCE(NEW.pago_otro, 0) ELSE 0 END
                               + CASE WHEN NEW.pago_efectivo_metodo IN ('TRANSFERENCIA', 'YAPE', 'PLIN') THEN COALESCE(NEW.pago_efectivo, 0) ELSE 0 END
                             ELSE 0 END,
    total_ventas         = total_ventas         + NEW.total,
    numero_transacciones = numero_transacciones + 1,
    cambio_total         = cambio_total         + COALESCE(NEW.cambio, 0),
    ticket_promedio      = (total_ventas + NEW.total) / (numero_transacciones + 1)
  WHERE id = (
    SELECT id FROM cajas
    WHERE estado = 'ABIERTA'
    ORDER BY (usuario_id = NEW.usuario_id) DESC, id DESC
    LIMIT 1
  );
END;

CREATE TRIGGER trg_actualizar_caja_cancelar_venta
AFTER UPDATE ON ventas
FOR EACH ROW
WHEN OLD.estado = 'COMPLETADA' AND NEW.estado = 'CANCELADA'
BEGIN
  UPDATE cajas
  SET
    ventas_efectivo      = ventas_efectivo - CASE
                             WHEN NEW.metodo_pago = 'EFECTIVO' THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' AND COALESCE(NEW.pago_efectivo_metodo, 'EFECTIVO') = 'EFECTIVO'
                               THEN COALESCE(NEW.pago_efectivo, 0)
                             ELSE 0 END,
    ventas_tarjeta       = ventas_tarjeta - CASE
                             WHEN NEW.metodo_pago = 'TARJETA' THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' THEN
                                 CASE WHEN NEW.pago_otro_metodo = 'TARJETA' THEN COALESCE(NEW.pago_otro, 0) ELSE 0 END
                               + CASE WHEN NEW.pago_efectivo_metodo = 'TARJETA' THEN COALESCE(NEW.pago_efectivo, 0) ELSE 0 END
                             ELSE 0 END,
    ventas_transferencia = ventas_transferencia - CASE
                             WHEN NEW.metodo_pago IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' THEN
                                 CASE WHEN NEW.pago_otro_metodo IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN COALESCE(NEW.pago_otro, 0) ELSE 0 END
                               + CASE WHEN NEW.pago_efectivo_metodo IN ('TRANSFERENCIA', 'YAPE', 'PLIN') THEN COALESCE(NEW.pago_efectivo, 0) ELSE 0 END
                             ELSE 0 END,
    total_ventas         = total_ventas         - NEW.total,
    numero_transacciones = numero_transacciones - 1
  WHERE id = (
    SELECT id FROM cajas
    WHERE estado = 'ABIERTA'
    ORDER BY (usuario_id = NEW.usuario_id) DESC, id DESC
    LIMIT 1
  );
END;
