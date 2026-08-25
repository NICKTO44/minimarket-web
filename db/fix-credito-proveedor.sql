DROP TRIGGER IF EXISTS trg_credito_proveedor_devolucion;
CREATE TRIGGER trg_credito_proveedor_devolucion
AFTER UPDATE OF estado ON devoluciones_proveedor
FOR EACH ROW
WHEN NEW.estado = 'ACEPTADA' AND OLD.estado != 'ACEPTADA'
  AND NEW.tipo_resolucion = 'CREDITO'
BEGIN
  UPDATE proveedores
  SET credito_disponible = credito_disponible + NEW.monto_devolucion,
      fecha_actualizacion = datetime('now', 'localtime')
  WHERE id = NEW.proveedor_id;
END;
