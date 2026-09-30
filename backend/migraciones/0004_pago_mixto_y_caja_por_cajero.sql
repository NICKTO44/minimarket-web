-- Pago mixto (efectivo + un medio digital) y totales de caja por cajero.
--
-- 1) Tres columnas nuevas en ventas, solo se llenan cuando
--    metodo_pago = 'MIXTO' (el CHECK de la tabla ya acepta 'MIXTO'
--    desde el schema original, así que NO hay que reconstruir la tabla):
--      pago_efectivo    -> parte cobrada en efectivo (neta, sin vuelto)
--      pago_otro        -> parte cobrada por el otro medio
--      pago_otro_metodo -> TARJETA | TRANSFERENCIA | YAPE_PLIN
--    Para ventas normales quedan en NULL y todo funciona como antes.
--
-- 2) Se recrean los 3 triggers que actualizan los totales de la caja:
--    - reparten una venta MIXTO entre efectivo y el otro medio;
--    - actualizan SOLO una caja: la del cajero que hizo la operación
--      (antes se sumaba a TODAS las cajas abiertas, lo que duplicaba
--      las ventas si había dos cajeros con caja abierta a la vez). Si
--      ese usuario no tiene caja abierta (p. ej. un administrador
--      procesa una devolución), se usa la caja abierta más reciente.
--
-- ORDEN IMPORTANTE: primero las columnas, después los triggers. Un
-- trigger que menciona una columna inexistente haría fallar TODAS las
-- ventas nuevas. Ningún paso usa RENAME, así que no afecta a otros
-- triggers de la base.

ALTER TABLE ventas ADD COLUMN pago_efectivo REAL;
ALTER TABLE ventas ADD COLUMN pago_otro REAL;
ALTER TABLE ventas ADD COLUMN pago_otro_metodo TEXT;

DROP TRIGGER IF EXISTS trg_actualizar_caja_venta;
DROP TRIGGER IF EXISTS trg_actualizar_caja_cancelar_venta;
DROP TRIGGER IF EXISTS trg_actualizar_caja_devolucion;

CREATE TRIGGER trg_actualizar_caja_venta
AFTER INSERT ON ventas
FOR EACH ROW
WHEN NEW.estado = 'COMPLETADA'
BEGIN
  UPDATE cajas
  SET
    ventas_efectivo      = ventas_efectivo + CASE
                             WHEN NEW.metodo_pago = 'EFECTIVO' THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO'    THEN COALESCE(NEW.pago_efectivo, 0)
                             ELSE 0 END,
    ventas_tarjeta       = ventas_tarjeta + CASE
                             WHEN NEW.metodo_pago = 'TARJETA' THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' AND NEW.pago_otro_metodo = 'TARJETA' THEN COALESCE(NEW.pago_otro, 0)
                             ELSE 0 END,
    ventas_transferencia = ventas_transferencia + CASE
                             WHEN NEW.metodo_pago IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' AND NEW.pago_otro_metodo IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN COALESCE(NEW.pago_otro, 0)
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
                             WHEN NEW.metodo_pago = 'MIXTO'    THEN COALESCE(NEW.pago_efectivo, 0)
                             ELSE 0 END,
    ventas_tarjeta       = ventas_tarjeta - CASE
                             WHEN NEW.metodo_pago = 'TARJETA' THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' AND NEW.pago_otro_metodo = 'TARJETA' THEN COALESCE(NEW.pago_otro, 0)
                             ELSE 0 END,
    ventas_transferencia = ventas_transferencia - CASE
                             WHEN NEW.metodo_pago IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN NEW.total
                             WHEN NEW.metodo_pago = 'MIXTO' AND NEW.pago_otro_metodo IN ('TRANSFERENCIA', 'YAPE_PLIN') THEN COALESCE(NEW.pago_otro, 0)
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

CREATE TRIGGER trg_actualizar_caja_devolucion
AFTER INSERT ON devoluciones
FOR EACH ROW
WHEN NEW.estado = 'PROCESADA'
BEGIN
  UPDATE cajas
  SET
    devoluciones_monto    = devoluciones_monto + NEW.monto_reembolsado,
    devoluciones_cantidad = devoluciones_cantidad + 1,
    ventas_efectivo       = ventas_efectivo      - CASE WHEN NEW.metodo_reembolso = 'EFECTIVO'      THEN NEW.monto_reembolsado ELSE 0 END,
    ventas_tarjeta        = ventas_tarjeta       - CASE WHEN NEW.metodo_reembolso = 'TARJETA'       THEN NEW.monto_reembolsado ELSE 0 END,
    ventas_transferencia  = ventas_transferencia - CASE WHEN NEW.metodo_reembolso = 'TRANSFERENCIA' THEN NEW.monto_reembolsado ELSE 0 END
  WHERE id = (
    SELECT id FROM cajas
    WHERE estado = 'ABIERTA'
    ORDER BY (usuario_id = NEW.usuario_id) DESC, id DESC
    LIMIT 1
  );
END;
