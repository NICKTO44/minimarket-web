-- Módulo "Cafetería / Restaurante" (atención en mesas) -- Fase 1.
--
-- Todo es ADITIVO: solo ADD COLUMN, tablas nuevas e índices. No se
-- recrea ni se renombra nada, así que los negocios que ya existen
-- (bodegas, minimarkets) siguen funcionando exactamente igual: las
-- tablas nuevas quedan vacías y el módulo apagado (modo_negocio =
-- 'TIENDA') hasta que el administrador lo active.

-- 1) Tipo de negocio. 'TIENDA' = sistema de siempre.
--    'RESTAURANTE' = activa mesas, pedidos, comandas y modificadores.
ALTER TABLE configuracion_tienda ADD COLUMN modo_negocio TEXT DEFAULT 'TIENDA';

-- 2) Productos preparados al momento (un café, un jugo) no llevan stock.
--    1 = controla stock (como siempre), 0 = se vende sin descontar stock.
ALTER TABLE productos ADD COLUMN controla_stock INTEGER NOT NULL DEFAULT 1;

-- 3) Venta que salió de un pedido de mesa (NULL en ventas normales).
ALTER TABLE ventas ADD COLUMN pedido_id INTEGER;

-- 4) Rol Mesero: toma pedidos, pero no cobra ni abre caja.
INSERT INTO roles (nombre, descripcion)
SELECT 'MESERO', 'Toma pedidos en mesas; no cobra ni maneja caja'
WHERE NOT EXISTS (SELECT 1 FROM roles WHERE nombre = 'MESERO');

-- 5) Mesas del local, agrupadas por zona (Salón, Terraza, Barra...).
CREATE TABLE IF NOT EXISTS mesas (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL,
  zona TEXT NOT NULL DEFAULT 'Salón',
  capacidad INTEGER DEFAULT 4,
  orden INTEGER DEFAULT 0,
  activo INTEGER NOT NULL DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime'))
);

-- 6) Pedidos abiertos (cuentas). Se cobran con el POS de siempre.
CREATE TABLE IF NOT EXISTS pedidos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tipo TEXT NOT NULL DEFAULT 'MESA' CHECK (tipo IN ('MESA', 'LLEVAR', 'DELIVERY')),
  mesa_id INTEGER,
  cliente_nombre TEXT,
  personas INTEGER,
  estado TEXT NOT NULL DEFAULT 'ABIERTO' CHECK (estado IN ('ABIERTO', 'COBRADO', 'ANULADO')),
  usuario_id INTEGER NOT NULL,
  notas TEXT,
  venta_id INTEGER,
  motivo_anulacion TEXT,
  fecha_apertura TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_cierre TEXT,
  FOREIGN KEY (mesa_id) REFERENCES mesas(id),
  FOREIGN KEY (usuario_id) REFERENCES usuarios(id),
  FOREIGN KEY (venta_id) REFERENCES ventas(id)
);

-- Una mesa solo puede tener UN pedido abierto a la vez.
CREATE UNIQUE INDEX IF NOT EXISTS idx_pedidos_mesa_abierta
  ON pedidos(mesa_id) WHERE estado = 'ABIERTO' AND mesa_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pedidos_estado ON pedidos(estado);

-- 7) Líneas del pedido. PENDIENTE = aún no se mandó a preparar;
--    ENVIADO = ya salió en una comanda; ANULADO = se quitó (queda el rastro).
CREATE TABLE IF NOT EXISTS pedido_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pedido_id INTEGER NOT NULL,
  producto_id INTEGER NOT NULL,
  nombre_producto TEXT NOT NULL,
  opciones TEXT,
  nota TEXT,
  cantidad REAL NOT NULL CHECK (cantidad > 0),
  precio_unitario REAL NOT NULL CHECK (precio_unitario >= 0),
  estado TEXT NOT NULL DEFAULT 'PENDIENTE' CHECK (estado IN ('PENDIENTE', 'ENVIADO', 'ANULADO')),
  usuario_id INTEGER,
  motivo_anulacion TEXT,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_envio TEXT,
  FOREIGN KEY (pedido_id) REFERENCES pedidos(id),
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);
CREATE INDEX IF NOT EXISTS idx_pedido_items_pedido ON pedido_items(pedido_id);

-- 8) Modificadores: grupos (Tamaño, Tipo de leche, Extras) con sus
--    opciones y precio extra, y a qué productos se aplican.
CREATE TABLE IF NOT EXISTS grupos_modificadores (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL,
  obligatorio INTEGER NOT NULL DEFAULT 0,
  multiple INTEGER NOT NULL DEFAULT 0,
  orden INTEGER DEFAULT 0,
  activo INTEGER NOT NULL DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE IF NOT EXISTS opciones_modificador (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  grupo_id INTEGER NOT NULL,
  nombre TEXT NOT NULL,
  precio_extra REAL NOT NULL DEFAULT 0 CHECK (precio_extra >= 0),
  orden INTEGER DEFAULT 0,
  activo INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (grupo_id) REFERENCES grupos_modificadores(id)
);
CREATE INDEX IF NOT EXISTS idx_opciones_grupo ON opciones_modificador(grupo_id);

CREATE TABLE IF NOT EXISTS producto_grupos_modificador (
  producto_id INTEGER NOT NULL,
  grupo_id INTEGER NOT NULL,
  PRIMARY KEY (producto_id, grupo_id),
  FOREIGN KEY (producto_id) REFERENCES productos(id),
  FOREIGN KEY (grupo_id) REFERENCES grupos_modificadores(id)
);
