-- =====================================================
-- MINIMARKET-WEB — Base de Datos SQLite/Turso
-- Reconstruido el 2026-08-28 a partir del .schema real de
-- producción (minimarket-web), que había recibido varias
-- migraciones manuales (db/fix-*.sql, db/agregar-*.sql)
-- nunca reflejadas de vuelta en este archivo. Esta versión
-- SÍ es la fuente de verdad de ahora en adelante — es la
-- que se incrusta en el binario (include_str!) y se corre
-- automáticamente sobre cada base nueva que se crea en /registro.
-- =====================================================

PRAGMA foreign_keys = OFF;

-- =====================================================
-- TABLA: roles
-- =====================================================
CREATE TABLE roles (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL UNIQUE,
  descripcion TEXT,
  permisos TEXT,
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX idx_roles_nombre ON roles(nombre);
CREATE INDEX idx_roles_activo ON roles(activo);

-- =====================================================
-- TABLA: usuarios
-- =====================================================
CREATE TABLE usuarios (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  username TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  nombre_completo TEXT NOT NULL,
  email TEXT,
  rol_id INTEGER NOT NULL,
  activo INTEGER DEFAULT 1,
  intentos_fallidos INTEGER DEFAULT 0,
  bloqueado_hasta TEXT,
  ultimo_acceso TEXT,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime')),
  FOREIGN KEY (rol_id) REFERENCES roles(id)
);

CREATE INDEX idx_usuarios_username ON usuarios(username);
CREATE INDEX idx_usuarios_activo ON usuarios(activo);
CREATE INDEX idx_usuarios_rol ON usuarios(rol_id);

-- =====================================================
-- TABLA: sesiones_log
-- =====================================================
CREATE TABLE sesiones_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  usuario_id INTEGER NOT NULL,
  fecha_hora TEXT DEFAULT (datetime('now', 'localtime')),
  ip_address TEXT,
  user_agent TEXT,
  resultado TEXT NOT NULL CHECK(resultado IN ('EXITOSO', 'FALLIDO', 'BLOQUEADO')),
  motivo_fallo TEXT,
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id) ON DELETE CASCADE
);

CREATE INDEX idx_sesiones_usuario ON sesiones_log(usuario_id);
CREATE INDEX idx_sesiones_fecha ON sesiones_log(fecha_hora);

-- =====================================================
-- TABLA: clientes
-- (CHECK ampliado con CE/PASAPORTE respecto al original)
-- =====================================================
CREATE TABLE clientes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tipo_documento TEXT NOT NULL DEFAULT 'DNI' CHECK (tipo_documento IN ('DNI', 'CE', 'PASAPORTE', 'RUC', 'SIN_DOCUMENTO')),
  numero_documento TEXT,
  nombre_razon_social TEXT NOT NULL,
  telefono TEXT,
  email TEXT,
  direccion TEXT,
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX idx_clientes_documento ON clientes(numero_documento);
CREATE INDEX idx_clientes_nombre ON clientes(nombre_razon_social);

-- =====================================================
-- TABLA: categorias
-- =====================================================
CREATE TABLE categorias (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL UNIQUE,
  descripcion TEXT,
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX idx_categorias_nombre ON categorias(nombre);
CREATE INDEX idx_categorias_activo ON categorias(activo);

-- =====================================================
-- TABLA: productos
-- (con precio_compra, agregado por migración manual)
-- =====================================================
CREATE TABLE productos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  codigo TEXT NOT NULL UNIQUE,
  nombre TEXT NOT NULL,
  descripcion TEXT,
  precio REAL NOT NULL CHECK (precio > 0),
  stock REAL NOT NULL DEFAULT 0 CHECK (stock >= 0),
  stock_minimo REAL DEFAULT 5,
  unidad_medida TEXT NOT NULL DEFAULT 'UNIDAD' CHECK (unidad_medida IN ('UNIDAD', 'KG', 'GRAMO', 'LITRO', 'ML', 'PAQUETE')),
  categoria_id INTEGER NOT NULL,
  descuento_porcentaje REAL DEFAULT 0,
  lleva_vencimiento INTEGER DEFAULT 0,
  imagen_url TEXT,
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime')),
  precio_compra REAL DEFAULT 0,
  FOREIGN KEY (categoria_id) REFERENCES categorias(id)
);

CREATE INDEX idx_productos_codigo ON productos(codigo);
CREATE INDEX idx_productos_nombre ON productos(nombre);
CREATE INDEX idx_productos_categoria ON productos(categoria_id);
CREATE INDEX idx_productos_activo ON productos(activo);

-- =====================================================
-- TABLA: lotes_producto — FEFO
-- =====================================================
CREATE TABLE lotes_producto (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad >= 0),
  fecha_vencimiento TEXT NOT NULL,
  fecha_ingreso TEXT DEFAULT (datetime('now', 'localtime')),
  compra_id INTEGER,
  numero_lote TEXT,
  activo INTEGER DEFAULT 1,
  FOREIGN KEY (producto_id) REFERENCES productos(id) ON DELETE CASCADE,
  FOREIGN KEY (compra_id) REFERENCES compras(id)
);

CREATE INDEX idx_lotes_producto ON lotes_producto(producto_id);
CREATE INDEX idx_lotes_vencimiento ON lotes_producto(fecha_vencimiento);

CREATE TRIGGER trg_sync_stock_lote_insert
AFTER INSERT ON lotes_producto
BEGIN
  UPDATE productos SET stock = (
    SELECT COALESCE(SUM(cantidad), 0) FROM lotes_producto
    WHERE producto_id = NEW.producto_id AND activo = 1
  )
  WHERE id = NEW.producto_id;
END;

CREATE TRIGGER trg_sync_stock_lote_update
AFTER UPDATE OF cantidad, activo ON lotes_producto
BEGIN
  UPDATE productos SET stock = (
    SELECT COALESCE(SUM(cantidad), 0) FROM lotes_producto
    WHERE producto_id = NEW.producto_id AND activo = 1
  )
  WHERE id = NEW.producto_id;
END;

-- =====================================================
-- TABLA: ventas
-- =====================================================
CREATE TABLE ventas (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  folio TEXT NOT NULL UNIQUE,
  fecha_hora TEXT DEFAULT (datetime('now', 'localtime')),
  cliente_id INTEGER,
  subtotal REAL NOT NULL CHECK (subtotal >= 0),
  descuento REAL DEFAULT 0 CHECK (descuento >= 0),
  total REAL NOT NULL CHECK (total >= 0),
  metodo_pago TEXT NOT NULL CHECK(metodo_pago IN ('EFECTIVO', 'TARJETA', 'TRANSFERENCIA', 'YAPE_PLIN', 'MIXTO')),
  monto_recibido REAL,
  cambio REAL,
  usuario_id INTEGER NOT NULL,
  estado TEXT DEFAULT 'COMPLETADA' CHECK(estado IN ('COMPLETADA', 'CANCELADA', 'PENDIENTE')),
  motivo_cancelacion TEXT,
  notas TEXT,
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id),
  FOREIGN KEY (cliente_id) REFERENCES clientes(id)
);

CREATE INDEX idx_ventas_folio ON ventas(folio);
CREATE INDEX idx_ventas_fecha ON ventas(fecha_hora);
CREATE INDEX idx_ventas_usuario ON ventas(usuario_id);
CREATE INDEX idx_ventas_estado ON ventas(estado);
CREATE INDEX idx_ventas_metodo ON ventas(metodo_pago);
CREATE INDEX idx_ventas_cliente ON ventas(cliente_id);

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

-- =====================================================
-- TABLA: detalles_venta
-- =====================================================
CREATE TABLE detalles_venta (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0),
  precio_unitario REAL NOT NULL CHECK (precio_unitario >= 0),
  subtotal REAL NOT NULL CHECK (subtotal >= 0),
  descuento_linea REAL DEFAULT 0,
  total_linea REAL NOT NULL CHECK (total_linea >= 0),
  FOREIGN KEY (venta_id) REFERENCES ventas(id) ON DELETE CASCADE,
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);

CREATE INDEX idx_detalles_venta ON detalles_venta(venta_id);
CREATE INDEX idx_detalles_producto ON detalles_venta(producto_id);

-- =====================================================
-- TABLA: comprobantes_electronicos
-- (con enlace_cdr, external_id, hash — agregados por migración
--  al pasar de NubeFacT a FacturaLibre)
-- =====================================================
CREATE TABLE comprobantes_electronicos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  venta_id INTEGER NOT NULL,
  tipo TEXT NOT NULL CHECK (tipo IN ('BOLETA', 'FACTURA')),
  proveedor TEXT DEFAULT 'MOCK',
  serie TEXT,
  numero INTEGER,
  cliente_documento TEXT,
  cliente_nombre TEXT,
  estado TEXT NOT NULL DEFAULT 'PENDIENTE' CHECK (estado IN ('PENDIENTE', 'ACEPTADO', 'RECHAZADO', 'ERROR')),
  mensaje_sunat TEXT,
  enlace_pdf TEXT,
  enlace_xml TEXT,
  fecha_emision TEXT DEFAULT (datetime('now', 'localtime')),
  enlace_cdr TEXT,
  external_id TEXT,
  hash TEXT,
  FOREIGN KEY (venta_id) REFERENCES ventas(id)
);

CREATE INDEX idx_comprobantes_venta ON comprobantes_electronicos(venta_id);

-- =====================================================
-- TABLA: devoluciones (clientes)
-- =====================================================
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
    ventas_tarjeta        = ventas_tarjeta       - CASE WHEN NEW.metodo_reembolso = 'TARJETA'       THEN NEW.monto_reembolsado ELSE 0 END,
    ventas_transferencia  = ventas_transferencia  - CASE WHEN NEW.metodo_reembolso = 'TRANSFERENCIA' THEN NEW.monto_reembolsado ELSE 0 END
  WHERE estado = 'ABIERTA';
END;

-- =====================================================
-- TABLA: detalles_devolucion (clientes)
-- =====================================================
CREATE TABLE detalles_devolucion (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  devolucion_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  detalle_venta_id INTEGER,
  venta_id INTEGER NOT NULL,
  cantidad_devuelta REAL NOT NULL CHECK (cantidad_devuelta > 0),
  precio_unitario REAL NOT NULL,
  subtotal REAL NOT NULL CHECK (subtotal >= 0),
  condicion TEXT NOT NULL CHECK(condicion IN ('REVENTA', 'DEFECTUOSO', 'VENCIDO')),
  FOREIGN KEY (devolucion_id) REFERENCES devoluciones(id) ON DELETE CASCADE,
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);

CREATE INDEX idx_detalles_devolucion ON detalles_devolucion(devolucion_id);
CREATE INDEX idx_detalles_devolucion_producto ON detalles_devolucion(producto_id);

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

-- =====================================================
-- TABLA: movimientos_inventario
-- =====================================================
CREATE TABLE movimientos_inventario (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  tipo_movimiento TEXT NOT NULL CHECK(tipo_movimiento IN ('VENTA', 'DEVOLUCION', 'ENTRADA', 'SALIDA', 'AJUSTE', 'MERMA')),
  cantidad REAL NOT NULL,
  stock_anterior REAL NOT NULL,
  stock_nuevo REAL NOT NULL,
  venta_id INTEGER,
  devolucion_id INTEGER,
  compra_id INTEGER,
  usuario_id INTEGER,
  referencia TEXT,
  motivo TEXT,
  fecha_hora TEXT DEFAULT (datetime('now', 'localtime')),
  FOREIGN KEY (producto_id) REFERENCES productos(id),
  FOREIGN KEY (venta_id) REFERENCES ventas(id) ON DELETE SET NULL,
  FOREIGN KEY (devolucion_id) REFERENCES devoluciones(id) ON DELETE SET NULL,
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id)
);

CREATE INDEX idx_movimientos_inventario_producto ON movimientos_inventario(producto_id);
CREATE INDEX idx_movimientos_inventario_tipo ON movimientos_inventario(tipo_movimiento);
CREATE INDEX idx_movimientos_inventario_fecha ON movimientos_inventario(fecha_hora);

-- =====================================================
-- TABLA: proveedores
-- =====================================================
CREATE TABLE proveedores (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL,
  contacto TEXT,
  telefono TEXT,
  email TEXT,
  direccion TEXT,
  tipo_documento TEXT DEFAULT 'RUC' CHECK(tipo_documento IN ('RUC', 'DNI', 'NINGUNO')),
  numero_documento TEXT,
  banco TEXT,
  numero_cuenta TEXT,
  notas TEXT,
  total_compras REAL DEFAULT 0,
  credito_disponible REAL DEFAULT 0 CHECK (credito_disponible >= 0),
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX idx_proveedores_nombre ON proveedores(nombre);
CREATE INDEX idx_proveedores_activo ON proveedores(activo);
CREATE INDEX idx_proveedores_documento ON proveedores(numero_documento);

-- =====================================================
-- TABLA: compras
-- =====================================================
CREATE TABLE compras (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  folio TEXT NOT NULL UNIQUE,
  proveedor_id INTEGER NOT NULL,
  fecha_compra TEXT NOT NULL,
  fecha_recepcion TEXT,
  subtotal REAL NOT NULL DEFAULT 0 CHECK (subtotal >= 0),
  descuento REAL DEFAULT 0 CHECK (descuento >= 0),
  credito_aplicado REAL DEFAULT 0 CHECK (credito_aplicado >= 0),
  total REAL NOT NULL CHECK (total >= 0),
  tipo_pago TEXT DEFAULT 'EFECTIVO' CHECK(tipo_pago IN ('EFECTIVO', 'TRANSFERENCIA', 'CREDITO', 'MIXTO')),
  monto_pagado REAL DEFAULT 0 CHECK (monto_pagado >= 0),
  saldo_pendiente REAL DEFAULT 0 CHECK (saldo_pendiente >= 0),
  fecha_vencimiento_pago TEXT,
  estado TEXT DEFAULT 'PENDIENTE' CHECK(estado IN ('PENDIENTE', 'RECIBIDA', 'PARCIAL', 'CANCELADA')),
  estado_pago TEXT DEFAULT 'PENDIENTE' CHECK(estado_pago IN ('PENDIENTE', 'PARCIAL', 'PAGADO')),
  usuario_id INTEGER NOT NULL,
  factura_numero TEXT,
  notas TEXT,
  notas_recepcion TEXT,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime')),
  FOREIGN KEY (proveedor_id) REFERENCES proveedores(id),
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id)
);

CREATE INDEX idx_compras_folio ON compras(folio);
CREATE INDEX idx_compras_proveedor ON compras(proveedor_id);
CREATE INDEX idx_compras_fecha ON compras(fecha_compra);
CREATE INDEX idx_compras_estado ON compras(estado);
CREATE INDEX idx_compras_estado_pago ON compras(estado_pago);

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

-- =====================================================
-- TABLA: detalles_compra
-- =====================================================
CREATE TABLE detalles_compra (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  compra_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  cantidad REAL NOT NULL CHECK (cantidad > 0),
  cantidad_recibida REAL DEFAULT 0 CHECK (cantidad_recibida >= 0),
  cantidad_conforme REAL DEFAULT 0 CHECK (cantidad_conforme >= 0),
  precio_compra REAL NOT NULL CHECK (precio_compra >= 0),
  precio_venta_sugerido REAL,
  subtotal REAL NOT NULL CHECK (subtotal >= 0),
  FOREIGN KEY (compra_id) REFERENCES compras(id) ON DELETE CASCADE,
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);

CREATE INDEX idx_detalles_compra ON detalles_compra(compra_id);
CREATE INDEX idx_detalles_compra_producto ON detalles_compra(producto_id);

-- =====================================================
-- TABLA: pagos_compra
-- =====================================================
CREATE TABLE pagos_compra (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  compra_id INTEGER NOT NULL,
  monto REAL NOT NULL CHECK (monto > 0),
  fecha_pago TEXT DEFAULT (datetime('now', 'localtime')),
  metodo_pago TEXT NOT NULL CHECK(metodo_pago IN ('EFECTIVO', 'TRANSFERENCIA', 'CHEQUE', 'OTRO')),
  referencia TEXT,
  notas TEXT,
  usuario_id INTEGER NOT NULL,
  FOREIGN KEY (compra_id) REFERENCES compras(id) ON DELETE CASCADE,
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id)
);

CREATE INDEX idx_pagos_compra ON pagos_compra(compra_id);
CREATE INDEX idx_pagos_compra_fecha ON pagos_compra(fecha_pago);

-- =====================================================
-- TABLA: devoluciones_proveedor
-- =====================================================
CREATE TABLE devoluciones_proveedor (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  compra_id INTEGER NOT NULL,
  proveedor_id INTEGER NOT NULL,
  folio TEXT NOT NULL UNIQUE,
  fecha TEXT DEFAULT (datetime('now', 'localtime')),
  motivo TEXT NOT NULL CHECK(motivo IN (
    'DAÑADO', 'DEFECTUOSO', 'PRODUCTO_INCORRECTO', 'VENCIDO', 'OTRO'
  )),
  detalle_motivo TEXT,
  monto_devolucion REAL NOT NULL CHECK (monto_devolucion > 0),
  estado TEXT DEFAULT 'PENDIENTE' CHECK(estado IN ('PENDIENTE', 'ACEPTADA', 'RECHAZADA')),
  tipo_resolucion TEXT CHECK(tipo_resolucion IN ('CREDITO', 'REEMBOLSO', 'CAMBIO')),
  usuario_id INTEGER NOT NULL,
  notas TEXT,
  fecha_resolucion TEXT,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  FOREIGN KEY (compra_id) REFERENCES compras(id),
  FOREIGN KEY (proveedor_id) REFERENCES proveedores(id),
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id)
);

CREATE INDEX idx_devprov_compra ON devoluciones_proveedor(compra_id);
CREATE INDEX idx_devprov_proveedor ON devoluciones_proveedor(proveedor_id);
CREATE INDEX idx_devprov_estado ON devoluciones_proveedor(estado);
CREATE INDEX idx_devprov_fecha ON devoluciones_proveedor(fecha);

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

-- =====================================================
-- TABLA: detalles_devolucion_proveedor
-- =====================================================
CREATE TABLE detalles_devolucion_proveedor (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  devolucion_proveedor_id INTEGER NOT NULL,
  detalle_compra_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  cantidad_devuelta INTEGER NOT NULL CHECK (cantidad_devuelta > 0),
  precio_compra REAL NOT NULL,
  subtotal REAL NOT NULL,
  motivo_item TEXT,
  FOREIGN KEY (devolucion_proveedor_id) REFERENCES devoluciones_proveedor(id) ON DELETE CASCADE,
  FOREIGN KEY (detalle_compra_id) REFERENCES detalles_compra(id),
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);

CREATE INDEX idx_detdevprov_devolucion ON detalles_devolucion_proveedor(devolucion_proveedor_id);
CREATE INDEX idx_detdevprov_producto ON detalles_devolucion_proveedor(producto_id);

-- =====================================================
-- TABLA: configuracion_tienda
-- (con facturalibre_token, facturalibre_ruta,
--  codigo_producto_sunat_generico — agregados por migración)
-- =====================================================
CREATE TABLE configuracion_tienda (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre_tienda TEXT NOT NULL,
  direccion TEXT,
  telefono TEXT,
  email TEXT,
  ruc TEXT,
  logo_path TEXT,
  mensaje_recibo TEXT,
  moneda TEXT DEFAULT 'PEN',
  formato_folio TEXT DEFAULT 'V-{YYYY}{MM}{DD}-{####}',
  iva_porcentaje REAL DEFAULT 18,
  dias_devolucion INTEGER DEFAULT 7,
  backup_automatico INTEGER DEFAULT 1,
  hora_backup TEXT DEFAULT '23:00:00',
  impresora_ip TEXT DEFAULT '',
  impresora_tipo TEXT DEFAULT 'TERMICA',
  impresora_puerto INTEGER DEFAULT 9100,
  facturacion_proveedor TEXT DEFAULT 'MOCK',
  facturacion_credenciales TEXT,
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime')),
  facturalibre_token TEXT,
  facturalibre_ruta TEXT,
  codigo_producto_sunat_generico TEXT DEFAULT '50000000'
);

-- =====================================================
-- TABLA: auditoria
-- =====================================================
CREATE TABLE auditoria (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  usuario_id INTEGER,
  accion TEXT NOT NULL,
  tabla_afectada TEXT,
  registro_id INTEGER,
  valores_anteriores TEXT,
  valores_nuevos TEXT,
  ip_address TEXT,
  fecha_hora TEXT DEFAULT (datetime('now', 'localtime')),
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id) ON DELETE SET NULL
);

CREATE INDEX idx_auditoria_usuario ON auditoria(usuario_id);
CREATE INDEX idx_auditoria_tabla ON auditoria(tabla_afectada);
CREATE INDEX idx_auditoria_fecha ON auditoria(fecha_hora);

-- =====================================================
-- CAJAS
-- =====================================================
CREATE TABLE turnos_configuracion (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL UNIQUE CHECK(nombre IN ('GENERAL')),
  hora_inicio_esperada TEXT NOT NULL,
  hora_fin_esperada TEXT NOT NULL,
  tolerancia_minutos INTEGER DEFAULT 15,
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX idx_turnos_nombre ON turnos_configuracion(nombre);

CREATE TABLE cajas (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  usuario_id INTEGER NOT NULL,
  numero_caja INTEGER DEFAULT 1,
  turno TEXT NOT NULL DEFAULT 'GENERAL' CHECK(turno IN ('GENERAL')),
  fecha_apertura TEXT DEFAULT (datetime('now', 'localtime')),
  hora_apertura TEXT DEFAULT (strftime('%H:%M:%S', 'now', 'localtime')),
  monto_inicial REAL NOT NULL CHECK (monto_inicial >= 0),
  observaciones_apertura TEXT,
  hora_esperada_inicio TEXT,
  minutos_retraso INTEGER DEFAULT 0,
  llego_tarde INTEGER DEFAULT 0,
  fecha_cierre TEXT,
  hora_cierre TEXT,
  hora_esperada_fin TEXT,
  monto_final_contado REAL,
  observaciones_cierre TEXT,
  desglose_efectivo TEXT,
  ventas_efectivo REAL DEFAULT 0,
  ventas_tarjeta REAL DEFAULT 0,
  ventas_transferencia REAL DEFAULT 0,
  total_ventas REAL DEFAULT 0,
  numero_transacciones INTEGER DEFAULT 0,
  ticket_promedio REAL DEFAULT 0,
  devoluciones_monto REAL DEFAULT 0,
  devoluciones_cantidad INTEGER DEFAULT 0,
  retiros_total REAL DEFAULT 0,
  ingresos_total REAL DEFAULT 0,
  gastos_total REAL DEFAULT 0,
  cambio_total REAL DEFAULT 0,
  efectivo_esperado REAL,
  diferencia REAL,
  estado_diferencia TEXT CHECK(estado_diferencia IN ('SIN_DIFERENCIA', 'ACEPTABLE', 'SIGNIFICATIVA')),
  justificacion_diferencia TEXT,
  estado TEXT DEFAULT 'ABIERTA' CHECK(estado IN ('ABIERTA', 'CERRADA')),
  duracion_turno_minutos INTEGER,
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id)
);

CREATE INDEX idx_cajas_usuario ON cajas(usuario_id);
CREATE INDEX idx_cajas_fecha ON cajas(fecha_apertura);
CREATE INDEX idx_cajas_estado ON cajas(estado);

CREATE TABLE movimientos_caja (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  caja_id INTEGER NOT NULL,
  tipo TEXT NOT NULL CHECK(tipo IN ('RETIRO', 'INGRESO', 'GASTO')),
  monto REAL NOT NULL CHECK(monto > 0),
  motivo TEXT NOT NULL,
  autorizado_por INTEGER,
  nombre_autorizador TEXT,
  fecha_hora TEXT DEFAULT (datetime('now', 'localtime')),
  hora TEXT DEFAULT (strftime('%H:%M:%S', 'now', 'localtime')),
  usuario_id INTEGER NOT NULL,
  FOREIGN KEY (caja_id) REFERENCES cajas(id) ON DELETE CASCADE,
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id),
  FOREIGN KEY (autorizado_por) REFERENCES usuarios(id)
);

CREATE INDEX idx_movimientos_caja_caja_id ON movimientos_caja(caja_id);
CREATE INDEX idx_movimientos_caja_tipo ON movimientos_caja(tipo);
CREATE INDEX idx_movimientos_caja_fecha ON movimientos_caja(fecha_hora);

-- =====================================================
-- TABLA: licencias
-- =====================================================
CREATE TABLE licencias (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  fecha_instalacion TEXT NOT NULL,
  fecha_primera_activacion TEXT,
  fecha_expiracion TEXT NOT NULL,
  fecha_ultimo_aviso TEXT,
  tipo_licencia TEXT NOT NULL CHECK(tipo_licencia IN ('TRIAL', 'MENSUAL', 'ANUAL', 'PERPETUA')),
  estado TEXT NOT NULL CHECK(estado IN ('ACTIVO', 'GRACIA', 'EXPIRADO', 'SUSPENDIDO')),
  codigo_activacion TEXT UNIQUE,
  codigo_usado INTEGER DEFAULT 0,
  nombre_cliente TEXT,
  email_cliente TEXT,
  telefono_cliente TEXT,
  intentos_activacion INTEGER DEFAULT 0,
  fecha_ultimo_intento TEXT,
  version_app TEXT,
  sistema_operativo TEXT,
  machine_id TEXT,
  total_ventas_realizadas INTEGER DEFAULT 0,
  total_productos_vendidos INTEGER DEFAULT 0,
  avisos_enviados INTEGER DEFAULT 0,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX idx_licencias_estado ON licencias(estado);
CREATE INDEX idx_licencias_tipo ON licencias(tipo_licencia);
CREATE INDEX idx_licencias_expiracion ON licencias(fecha_expiracion);
CREATE INDEX idx_licencias_codigo ON licencias(codigo_activacion);

-- =====================================================
-- DATOS INICIALES
-- =====================================================
INSERT INTO roles (nombre, descripcion) VALUES
('ADMIN', 'Acceso completo al sistema'),
('CAJERO', 'Acceso a punto de venta y caja'),
('INVENTARIO', 'Gestión de productos y proveedores');

INSERT INTO categorias (nombre, descripcion) VALUES
('Abarrotes', 'Productos secos: arroz, azúcar, menestras, fideos'),
('Bebidas', 'Gaseosas, jugos, agua, cerveza'),
('Lácteos', 'Leche, yogurt, queso, mantequilla'),
('Frutas y Verduras', 'Productos frescos'),
('Panadería', 'Pan y derivados'),
('Limpieza', 'Productos de limpieza del hogar'),
('Cuidado Personal', 'Higiene y cuidado personal'),
('Congelados', 'Productos que requieren congelación'),
('Snacks', 'Golosinas, galletas, piqueos'),
('Otros', 'Categoría general');

INSERT INTO configuracion_tienda (nombre_tienda, moneda, iva_porcentaje) VALUES
('Mi Minimarket', 'PEN', 18);

INSERT INTO turnos_configuracion (nombre, hora_inicio_esperada, hora_fin_esperada, tolerancia_minutos) VALUES
('GENERAL', '00:00:00', '23:59:59', 15);