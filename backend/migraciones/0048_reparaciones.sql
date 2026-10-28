-- Módulo REPARACIONES (rubro Tecnología / Reparaciones): órdenes de
-- reparación de celulares, laptops, impresoras... Ver handlers/reparaciones.rs.
--
-- Solo ADITIVO: una tabla nueva. Cada orden de reparación ES un encargo
-- (tabla encargos de la 0042): ahí están el cliente, la fecha prometida, el
-- presupuesto (encargo_items: repuestos del stock y mano de obra), los
-- adelantos (encargo_pagos) y el cobro en el punto de venta al entregar.
-- Esta tabla agrega lo propio del equipo y del taller.
--
--   reparaciones : una fila por orden (encargo_id).
--     tipo_equipo   : CELULAR, LAPTOP, PC, TABLET, IMPRESORA, CONSOLA u OTRO.
--     serie         : N.° de serie o IMEI (para la garantía y el historial).
--     accesorios    : lo que deja con el equipo (cargador, funda, chip...).
--     clave         : código o patrón de desbloqueo, solo si el cliente lo da.
--                     Se borra al entregar o cancelar la orden.
--     estado_fisico : cómo llega (rayones, pantalla rota, golpes).
--     falla         : lo que cuenta el cliente.
--     diagnostico   : lo que encuentra el técnico.
--     etapa         : RECIBIDO, DIAGNOSTICO, ESPERANDO_APROBACION,
--                     EN_REPARACION, LISTO o NO_REPARADO. El estado del
--                     encargo sigue a la etapa (PENDIENTE / EN_PREPARACION /
--                     LISTO); ENTREGADO y CANCELADO son del encargo.
--     tecnico       : quién la repara (nombre).
--     garantia_dias : días de garantía desde la entrega (0 = sin garantía).
--     aprobado_en   : cuándo el cliente aprobó el presupuesto.

CREATE TABLE IF NOT EXISTS reparaciones (
  encargo_id INTEGER PRIMARY KEY,
  tipo_equipo TEXT NOT NULL DEFAULT 'CELULAR'
    CHECK (tipo_equipo IN ('CELULAR', 'LAPTOP', 'PC', 'TABLET', 'IMPRESORA', 'CONSOLA', 'OTRO')),
  marca TEXT,
  modelo TEXT,
  serie TEXT,
  color TEXT,
  accesorios TEXT,
  clave TEXT,
  estado_fisico TEXT,
  falla TEXT NOT NULL,
  diagnostico TEXT,
  etapa TEXT NOT NULL DEFAULT 'RECIBIDO'
    CHECK (etapa IN ('RECIBIDO', 'DIAGNOSTICO', 'ESPERANDO_APROBACION', 'EN_REPARACION', 'LISTO', 'NO_REPARADO')),
  tecnico TEXT,
  garantia_dias INTEGER NOT NULL DEFAULT 0 CHECK (garantia_dias >= 0 AND garantia_dias <= 730),
  aprobado_en TEXT,
  fecha_actualizacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_reparaciones_serie ON reparaciones (serie);
CREATE INDEX IF NOT EXISTS idx_reparaciones_etapa ON reparaciones (etapa);
