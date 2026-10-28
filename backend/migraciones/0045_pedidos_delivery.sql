-- Módulo DELIVERY: pedidos por WhatsApp / delivery (dark kitchen).
--
-- Solo ADITIVO: una tabla nueva, 1 a 1 con `pedidos` (los pedidos tipo
-- 'DELIVERY' del restaurante, migración 0007). Los platos siguen en
-- `pedido_items`, la comanda y la cocina son las de siempre y el cobro es el
-- del punto de venta (POST /ventas con pedido_id). Aquí van los datos de la
-- entrega y el control del pago: adelantado (Yape/Plin antes de despachar) o
-- contra entrega (lo cobra el repartidor y lo rinde al volver).
-- Un negocio que no enciende el módulo no escribe nada en esta tabla.
CREATE TABLE IF NOT EXISTS pedido_delivery (
  pedido_id INTEGER PRIMARY KEY,
  cliente_id INTEGER,
  telefono TEXT,
  -- DELIVERY = se lleva a la dirección; RECOJO = el cliente lo recoge.
  entrega TEXT NOT NULL DEFAULT 'DELIVERY' CHECK (entrega IN ('DELIVERY', 'RECOJO')),
  direccion TEXT,
  referencia TEXT,
  -- Por dónde llegó: WHATSAPP, LLAMADA, REDES u OTRO.
  canal TEXT NOT NULL DEFAULT 'WHATSAPP',
  -- ADELANTADO = paga antes de despachar; CONTRA_ENTREGA = le paga al repartidor.
  pago TEXT NOT NULL DEFAULT 'CONTRA_ENTREGA' CHECK (pago IN ('ADELANTADO', 'CONTRA_ENTREGA')),
  -- Cómo dijo que va a pagar (EFECTIVO, YAPE, PLIN, TRANSFERENCIA, TARJETA).
  metodo_pago TEXT,
  -- Contra entrega en efectivo: con cuánto paga (el repartidor lleva el vuelto).
  paga_con REAL,
  costo_envio REAL NOT NULL DEFAULT 0 CHECK (costo_envio >= 0),
  repartidor TEXT,
  estado TEXT NOT NULL DEFAULT 'EN_COCINA' CHECK (estado IN ('EN_COCINA', 'EN_CAMINO', 'ENTREGADO')),
  fecha_salida TEXT,
  fecha_entrega TEXT,
  FOREIGN KEY (pedido_id) REFERENCES pedidos(id),
  FOREIGN KEY (cliente_id) REFERENCES clientes(id)
);

CREATE INDEX IF NOT EXISTS idx_pedido_delivery_estado ON pedido_delivery(estado);
CREATE INDEX IF NOT EXISTS idx_pedidos_tipo_estado ON pedidos(tipo, estado);
