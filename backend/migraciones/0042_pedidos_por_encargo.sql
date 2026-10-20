-- Módulo ENCARGOS (rubro Pastelería / Panadería): pedidos por encargo.
-- El cliente pide una torta para una fecha, deja un adelanto y la recoge
-- (o se le lleva) ese día. Al entregarla se cobra en el punto de venta
-- como una venta normal por el TOTAL del pedido: ahí se emite la boleta o
-- factura completa y solo se cobra el saldo (ver handlers/encargos.rs).
--
-- El adelanto NO es una venta ni lleva comprobante: es dinero recibido
-- que entra a la caja (en efectivo, como ingreso) y queda en el historial
-- del pedido.
--
-- Solo ADITIVO: tres tablas nuevas. No toca ventas, stock ni caja: el
-- stock se descuenta recién al entregar (es una venta normal).
--
--   encargos : un pedido.
--     numero        : correlativo del negocio (Pedido N.° 15).
--     fecha_entrega : AAAA-MM-DD; hora_entrega HH:MM (opcional).
--     entrega       : RECOJO (en tienda) o DELIVERY (con direccion_entrega).
--     total         : suma de sus ítems (con el precio acordado).
--     adelantado    : suma de los adelantos recibidos.
--     devuelto      : lo devuelto al cliente al cancelar.
--     estado        : PENDIENTE, EN_PREPARACION, LISTO, ENTREGADO (tiene
--                     su venta) o CANCELADO.
--     foto_url      : foto de referencia del diseño (opcional, una).
--
--   encargo_items : lo que lleva el pedido.
--     precio_unitario : el acordado con el cliente (se cobra ese).
--     detalle         : sabor, relleno, cobertura, tamaño...
--
--   encargo_pagos : adelantos (ADELANTO) y devoluciones (DEVOLUCION).
--     caja_id : la caja a la que entró o de la que salió (solo efectivo).

CREATE TABLE IF NOT EXISTS encargos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  numero INTEGER NOT NULL UNIQUE,
  cliente_id INTEGER,
  cliente_nombre TEXT NOT NULL,
  cliente_telefono TEXT,
  fecha_entrega TEXT NOT NULL,
  hora_entrega TEXT,
  entrega TEXT NOT NULL DEFAULT 'RECOJO' CHECK (entrega IN ('RECOJO', 'DELIVERY')),
  direccion_entrega TEXT,
  dedicatoria TEXT,
  notas TEXT,
  foto_url TEXT,
  total REAL NOT NULL DEFAULT 0 CHECK (total >= 0),
  adelantado REAL NOT NULL DEFAULT 0 CHECK (adelantado >= 0),
  devuelto REAL NOT NULL DEFAULT 0 CHECK (devuelto >= 0),
  estado TEXT NOT NULL DEFAULT 'PENDIENTE'
    CHECK (estado IN ('PENDIENTE', 'EN_PREPARACION', 'LISTO', 'ENTREGADO', 'CANCELADO')),
  venta_id INTEGER,
  usuario_id INTEGER,
  motivo_cancelacion TEXT,
  fecha_creacion TEXT,
  fecha_actualizacion TEXT,
  fecha_entregado TEXT
);

CREATE INDEX IF NOT EXISTS idx_encargos_estado_fecha ON encargos (estado, fecha_entrega);
CREATE INDEX IF NOT EXISTS idx_encargos_cliente ON encargos (cliente_id);

CREATE TABLE IF NOT EXISTS encargo_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  encargo_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  nombre TEXT NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0),
  precio_unitario REAL NOT NULL CHECK (precio_unitario >= 0),
  detalle TEXT
);

CREATE INDEX IF NOT EXISTS idx_encargo_items_encargo ON encargo_items (encargo_id);

CREATE TABLE IF NOT EXISTS encargo_pagos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  encargo_id INTEGER NOT NULL,
  tipo TEXT NOT NULL CHECK (tipo IN ('ADELANTO', 'DEVOLUCION')),
  monto REAL NOT NULL CHECK (monto > 0),
  metodo_pago TEXT NOT NULL,
  caja_id INTEGER,
  nota TEXT,
  usuario_id INTEGER,
  fecha TEXT
);

CREATE INDEX IF NOT EXISTS idx_encargo_pagos_encargo ON encargo_pagos (encargo_id);
